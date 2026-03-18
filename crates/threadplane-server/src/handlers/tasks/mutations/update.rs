use super::*;

async fn persist_task_update(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    request: &UpdateTaskRequest,
    current_transclusion_id: Option<Uuid>,
    updated_at: DateTime<Utc>,
) -> ServerResult<Option<Uuid>> {
    if let Some(transclusion_id) = current_transclusion_id {
        update_transclusion_group(
            tx,
            transclusion_id,
            &request.workspace,
            &request.actor,
            &request.title,
            &request.details,
            updated_at,
        )
        .await?;
        sync_transclusion_members(tx, transclusion_id).await?;
        sqlx::query(
            "
            UPDATE tasks
            SET epic_id = COALESCE($2, epic_id),
                priority = $3,
                owner = $4,
                labels = $5,
                updated_at = $6
            WHERE task_id = $1
            ",
        )
        .bind(request.task_id)
        .bind(request.epic_id)
        .bind(request.metadata.priority.to_string())
        .bind(request.metadata.owner.clone())
        .bind(request.metadata.labels.clone())
        .bind(updated_at)
        .execute(&mut **tx)
        .await?;
        return Ok(Some(transclusion_id));
    }

    sqlx::query(
        "
        UPDATE tasks
        SET title = $2,
            details = $3,
            epic_id = COALESCE($4, epic_id),
            priority = $5,
            owner = $6,
            labels = $7,
            updated_at = $8
        WHERE task_id = $1
        ",
    )
    .bind(request.task_id)
    .bind(&request.title)
    .bind(&request.details)
    .bind(request.epic_id)
    .bind(request.metadata.priority.to_string())
    .bind(request.metadata.owner.clone())
    .bind(request.metadata.labels.clone())
    .bind(updated_at)
    .execute(&mut **tx)
    .await?;

    Ok(None)
}

pub(crate) async fn update_task(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(mut request): Json<UpdateTaskRequest>,
) -> AppResult<TaskRecord> {
    request.metadata.labels = normalize_task_labels(request.metadata.labels);
    request.metadata.owner = normalize_task_owner(request.metadata.owner);
    require_workspace_editor()
        .pool(state.pool())
        .bootstrap(state.bootstrap())
        .workspace(&request.workspace)
        .actor(&request.actor)
        .call()
        .await?;
    ensure_supported_task_priority()
        .pool(state.pool())
        .bootstrap(state.bootstrap())
        .workspace(&request.workspace)
        .priority(&request.metadata.priority)
        .call()
        .await?;
    let mut tx = state.pool().begin().await?;
    let updated_at = Utc::now();
    let payload = serde_json::to_value(&request)?;
    let pending_receipt = match begin_idempotent_command::<TaskRecord>(
        &mut tx,
        IdempotencyContext {
            actor: &request.actor,
            command_kind: "update_task",
            idempotency_key: idempotency_key(&headers)?,
            request_payload: &payload,
            workspace: &request.workspace,
        },
        updated_at,
    )
    .await?
    {
        CommandExecution::Execute(pending_receipt) => pending_receipt,
        CommandExecution::Replay(envelope) => {
            tx.commit().await?;
            return Ok(Json(envelope));
        }
    };
    let task = fetch_task_by_id_tx(&mut tx, request.task_id, &request.workspace).await?;
    if let Some(epic_id) = request.epic_id {
        fetch_epic_by_id_tx(&mut tx, epic_id, &request.workspace).await?;
    }
    let event_id = append_event(
        &mut tx,
        &request.workspace,
        &request.actor,
        EventKind::TaskUpdated,
        &payload,
        updated_at,
    )
    .await?;

    let transclusion_id =
        persist_task_update(&mut tx, &request, task.transclusion_id, updated_at).await?;
    let record =
        TaskRecord::from(fetch_task_by_id_tx(&mut tx, request.task_id, &request.workspace).await?);
    let receipt =
        complete_idempotent_command(&mut tx, pending_receipt.as_ref(), &record, updated_at).await?;
    tx.commit().await?;

    project_graph_event()
        .pool(state.pool())
        .projection_coordinator(state.projection_coordinator())
        .workspace(&request.workspace)
        .event_id(event_id)
        .operation(Box::pin(async {
            if let Some(group_id) = transclusion_id {
                reproject_transclusion_group(state.graph(), state.pool(), group_id, None)
                    .await
                    .map_err(ThreadplaneServerError::internal)?;
                return Ok(());
            }

            project_task_record(state.graph(), state.pool(), &record).await
        }))
        .call()
        .await?;
    Ok(success_with_receipt(record, receipt))
}
