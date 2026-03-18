use super::shared::{persist_task_claim, project_claimed_task_record};
use super::*;

pub(crate) async fn claim_task(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<ClaimTaskRequest>,
) -> AppResult<TaskClaimRecord> {
    require_workspace_editor()
        .pool(state.pool())
        .bootstrap(state.bootstrap())
        .workspace(&request.workspace)
        .actor(&request.actor)
        .call()
        .await?;
    let lease_seconds =
        normalized_lease_seconds(request.lease_seconds, state.default_lease_seconds());
    let mut tx = state.pool().begin().await?;
    let claimed_at = Utc::now();
    let payload = serde_json::to_value(&request)?;
    let pending_receipt = match begin_idempotent_command::<TaskClaimRecord>(
        &mut tx,
        IdempotencyContext {
            actor: &request.actor,
            command_kind: "claim_task",
            idempotency_key: idempotency_key(&headers)?,
            request_payload: &payload,
            workspace: &request.workspace,
        },
        claimed_at,
    )
    .await?
    {
        CommandExecution::Execute(pending_receipt) => pending_receipt,
        CommandExecution::Replay(envelope) => {
            tx.commit().await?;
            return Ok(Json(envelope));
        }
    };

    let (record, task) = persist_task_claim()
        .tx(&mut tx)
        .workspace(&request.workspace)
        .actor(&request.actor)
        .task_id(request.task_id)
        .lease_seconds(lease_seconds)
        .claimed_at(claimed_at)
        .payload(&payload)
        .call()
        .await?;
    let receipt =
        complete_idempotent_command(&mut tx, pending_receipt.as_ref(), &record, claimed_at).await?;
    tx.commit().await?;
    let task_record = TaskRecord::from(task.clone());
    project_graph_event()
        .pool(state.pool())
        .projection_coordinator(state.projection_coordinator())
        .workspace(&record.workspace)
        .event_id(record.event_id)
        .operation(Box::pin(
            project_claimed_task_record()
                .graph(state.graph())
                .pool(state.pool())
                .task(&task)
                .task_record(&task_record)
                .claim(&record)
                .call(),
        ))
        .call()
        .await?;

    Ok(success_with_receipt(record, receipt))
}

pub(crate) async fn claim_next_task(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<ClaimNextTaskRequest>,
) -> AppResult<Option<TaskClaimRecord>> {
    require_workspace_editor()
        .pool(state.pool())
        .bootstrap(state.bootstrap())
        .workspace(&request.workspace)
        .actor(&request.actor)
        .call()
        .await?;
    if let Some(priority) = &request.priority {
        mutations::ensure_supported_task_priority()
            .pool(state.pool())
            .bootstrap(state.bootstrap())
            .workspace(&request.workspace)
            .priority(priority)
            .call()
            .await?;
    }
    let lease_seconds =
        normalized_lease_seconds(request.lease_seconds, state.default_lease_seconds());
    let filters = TaskListFilters {
        epic_id: request.epic_id,
        label: request.label.as_deref(),
        owner: request.owner.as_deref(),
        priority: request.priority.clone(),
        ready_only: true,
        status: Some("open"),
    };
    let candidates =
        fetch_tasks_for_listing(state.pool(), &request.workspace, filters, Some(25)).await?;

    for candidate in candidates {
        let mut tx = state.pool().begin().await?;
        let claimed_at = Utc::now();
        let payload = serde_json::to_value(&request)?;
        let pending_receipt = match begin_idempotent_command::<Option<TaskClaimRecord>>(
            &mut tx,
            IdempotencyContext {
                actor: &request.actor,
                command_kind: "claim_next_task",
                idempotency_key: idempotency_key(&headers)?,
                request_payload: &payload,
                workspace: &request.workspace,
            },
            claimed_at,
        )
        .await?
        {
            CommandExecution::Execute(pending_receipt) => pending_receipt,
            CommandExecution::Replay(envelope) => {
                tx.commit().await?;
                return Ok(Json(envelope));
            }
        };

        match persist_task_claim()
            .tx(&mut tx)
            .workspace(&request.workspace)
            .actor(&request.actor)
            .task_id(candidate.task_id)
            .lease_seconds(lease_seconds)
            .claimed_at(claimed_at)
            .payload(&payload)
            .call()
            .await
        {
            Ok((record, task)) => {
                let receipt = complete_idempotent_command(
                    &mut tx,
                    pending_receipt.as_ref(),
                    &Some(record.clone()),
                    claimed_at,
                )
                .await?;
                tx.commit().await?;
                let task_record = TaskRecord::from(task.clone());
                project_graph_event()
                    .pool(state.pool())
                    .projection_coordinator(state.projection_coordinator())
                    .workspace(&record.workspace)
                    .event_id(record.event_id)
                    .operation(Box::pin(
                        project_claimed_task_record()
                            .graph(state.graph())
                            .pool(state.pool())
                            .task(&task)
                            .task_record(&task_record)
                            .claim(&record)
                            .call(),
                    ))
                    .call()
                    .await?;
                return Ok(success_with_receipt(Some(record), receipt));
            }
            Err(ThreadplaneServerError::Conflict { .. }) => {
                tx.rollback().await?;
            }
            Err(error) => return Err(error),
        }
    }

    Ok(success(None))
}
