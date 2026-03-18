use super::*;

pub(crate) async fn create_epic(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateEpicRequest>,
) -> AppResult<EpicRecord> {
    require_workspace_editor()
        .pool(state.pool())
        .bootstrap(state.bootstrap())
        .workspace(&request.workspace)
        .actor(&request.author)
        .call()
        .await?;
    let mut tx = state.pool().begin().await?;
    let epic_id = Uuid::new_v4();
    let created_at = Utc::now();
    let payload = serde_json::to_value(&request)?;
    let pending_receipt = match begin_idempotent_command::<EpicRecord>(
        &mut tx,
        IdempotencyContext {
            actor: &request.author,
            command_kind: "create_epic",
            idempotency_key: idempotency_key(&headers)?,
            request_payload: &payload,
            workspace: &request.workspace,
        },
        created_at,
    )
    .await?
    {
        CommandExecution::Execute(pending_receipt) => pending_receipt,
        CommandExecution::Replay(envelope) => {
            tx.commit().await?;
            return Ok(Json(envelope));
        }
    };
    let event_id = append_event(
        &mut tx,
        &request.workspace,
        &request.author,
        EventKind::EpicRecorded,
        &payload,
        created_at,
    )
    .await?;

    sqlx::query(
        "
        INSERT INTO epics (
            epic_id,
            event_id,
            workspace,
            author,
            title,
            body,
            created_at,
            updated_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $7)
        ",
    )
    .bind(epic_id)
    .bind(event_id)
    .bind(&request.workspace)
    .bind(&request.author)
    .bind(&request.title)
    .bind(&request.body)
    .bind(created_at)
    .execute(&mut *tx)
    .await?;
    let record = EpicRecord::from(fetch_epic_by_id_tx(&mut tx, epic_id, &request.workspace).await?);
    let receipt =
        complete_idempotent_command(&mut tx, pending_receipt.as_ref(), &record, created_at).await?;
    tx.commit().await?;
    project_graph_event()
        .pool(state.pool())
        .projection_coordinator(state.projection_coordinator())
        .workspace(&record.workspace)
        .event_id(record.event_id)
        .operation(Box::pin(async {
            project_epic(state.graph(), &record)
                .await
                .map_err(ThreadplaneServerError::internal)
        }))
        .call()
        .await?;

    Ok(success_with_receipt(record, receipt))
}
