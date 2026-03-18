use super::*;

pub(crate) async fn add_task_dependency(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<AddTaskDependencyRequest>,
) -> AppResult<Value> {
    require_workspace_editor()
        .pool(state.pool())
        .bootstrap(state.bootstrap())
        .workspace(&request.workspace)
        .actor(&request.actor)
        .call()
        .await?;
    let mut tx = state.pool().begin().await?;
    let created_at = Utc::now();
    let payload = serde_json::to_value(&request)?;
    let pending_receipt = match begin_idempotent_command::<Value>(
        &mut tx,
        IdempotencyContext {
            actor: &request.actor,
            command_kind: "add_task_dependency",
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
    let event_id = append_task_dependency(
        &mut tx,
        &request.workspace,
        &request.actor,
        request.task_id,
        request.depends_on_task_id,
        created_at,
    )
    .await?;
    let data = json!({
        "task_id": request.task_id,
        "depends_on_task_id": request.depends_on_task_id,
        "relation": DEPENDS_ON_RELATION,
    });
    let receipt =
        complete_idempotent_command(&mut tx, pending_receipt.as_ref(), &data, created_at).await?;
    tx.commit().await?;

    project_graph_event()
        .pool(state.pool())
        .projection_coordinator(state.projection_coordinator())
        .workspace(&request.workspace)
        .event_id(event_id)
        .operation(Box::pin(async {
            project_task_dependency_by_id(
                state.graph(),
                state.pool(),
                request.task_id,
                request.depends_on_task_id,
            )
            .await
            .map_err(ThreadplaneServerError::internal)
        }))
        .call()
        .await?;

    Ok(success_with_receipt(data, receipt))
}
