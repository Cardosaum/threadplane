use super::*;

async fn project_memory_record(graph: &Graph, record: &MemoryRecord) -> ServerResult<()> {
    project_memory(graph, record).await.map_err(|error| {
        error!(?error, memory_id = %record.memory_id, "failed to project memory");
        ThreadplaneServerError::internal(error)
    })
}

pub(crate) async fn create_memory(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateMemoryRequest>,
) -> AppResult<MemoryRecord> {
    require_workspace_editor()
        .pool(state.pool())
        .bootstrap(state.bootstrap())
        .workspace(&request.workspace)
        .actor(&request.author)
        .call()
        .await?;
    let mut tx = state.pool().begin().await?;
    let memory_id = Uuid::new_v4();
    let created_at = Utc::now();
    let payload = serde_json::to_value(&request)?;
    let normalized_tags = normalize_memory_tags(request.tags.clone());
    let normalized_recall_triggers =
        normalize_memory_recall_triggers(request.recall_triggers.clone());
    let pending_receipt = match begin_idempotent_command::<MemoryRecord>(
        &mut tx,
        IdempotencyContext {
            actor: &request.author,
            command_kind: "create_memory",
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
        EventKind::MemoryRecorded,
        &payload,
        created_at,
    )
    .await?;

    sqlx::query(
        "
        INSERT INTO memories (
            memory_id,
            event_id,
            workspace,
            author,
            title,
            body,
            kind,
            scope,
            audience,
            importance,
            tags,
            recall_triggers,
            created_at,
            updated_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $13)
        ",
    )
    .bind(memory_id)
    .bind(event_id)
    .bind(&request.workspace)
    .bind(&request.author)
    .bind(&request.title)
    .bind(&request.body)
    .bind(request.kind.as_str())
    .bind(request.scope.to_string())
    .bind(request.audience.to_string())
    .bind(request.importance.to_string())
    .bind(&normalized_tags)
    .bind(&normalized_recall_triggers)
    .bind(created_at)
    .execute(&mut *tx)
    .await?;
    let record = MemoryRecord::try_from(
        fetch_memory_by_id_tx(&mut tx, memory_id, &request.workspace).await?,
    )?;
    let receipt =
        complete_idempotent_command(&mut tx, pending_receipt.as_ref(), &record, created_at).await?;
    tx.commit().await?;
    project_graph_event()
        .pool(state.pool())
        .projection_coordinator(state.projection_coordinator())
        .workspace(&record.workspace)
        .event_id(record.event_id)
        .operation(Box::pin(project_memory_record(state.graph(), &record)))
        .call()
        .await?;

    Ok(success_with_receipt(record, receipt))
}
