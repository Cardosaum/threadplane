#![allow(
    clippy::wildcard_imports,
    reason = "The task claim submodule intentionally builds on the task handler prelude."
)]

use super::*;

async fn ensure_task_is_unclaimed(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    task_id: Uuid,
) -> ServerResult<()> {
    let active_claim = fetch_active_claim_tx(tx, task_id).await?;
    if let Some(claim) = active_claim {
        return Err(ThreadplaneServerError::conflict(format!(
            "task already claimed by {} until {}",
            claim.actor,
            claim.expires_at.to_rfc3339()
        )));
    }

    Ok(())
}

#[builder]
async fn persist_task_claim(
    actor: &str,
    claimed_at: DateTime<Utc>,
    lease_seconds: i64,
    payload: &Value,
    task_id: Uuid,
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    workspace: &str,
) -> ServerResult<(TaskClaimRecord, TaskRow)> {
    fetch_task_by_id_tx(tx, task_id, workspace).await?;
    ensure_task_is_unclaimed(tx, task_id).await?;
    let expires_at = calculate_claim_expiry(claimed_at, lease_seconds)
        .ok_or_else(|| ThreadplaneServerError::bad_request("lease expiration overflow"))?;
    let event_id = append_event(
        tx,
        workspace,
        actor,
        EventKind::TaskClaimed,
        payload,
        claimed_at,
    )
    .await?;

    let claim_id = Uuid::new_v4();
    sqlx::query(
        "
        INSERT INTO task_claims (claim_id, task_id, workspace, actor, event_id, claimed_at, expires_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        ",
    )
    .bind(claim_id)
    .bind(task_id)
    .bind(workspace)
    .bind(actor)
    .bind(event_id)
    .bind(claimed_at)
    .bind(expires_at)
    .execute(&mut **tx)
    .await?;

    sqlx::query(
        "
        UPDATE tasks
        SET status = 'claimed',
            updated_at = $2
        WHERE task_id = $1
        ",
    )
    .bind(task_id)
    .bind(claimed_at)
    .execute(&mut **tx)
    .await?;

    let task = fetch_task_by_id_tx(tx, task_id, workspace).await?;
    let record = TaskClaimRecord {
        actor: actor.to_owned(),
        claim_id,
        claimed_at: claimed_at.to_rfc3339(),
        event_id,
        expires_at: expires_at.to_rfc3339(),
        task_id,
        workspace: workspace.to_owned(),
    };

    Ok((record, task))
}

#[builder]
async fn project_claimed_task_record(
    claim: &TaskClaimRecord,
    graph: &Graph,
    pool: &PgPool,
    task: &TaskRow,
    task_record: &TaskRecord,
) -> ServerResult<()> {
    mutations::project_task_record(graph, pool, task_record).await?;
    project_claim(graph, task, claim).await.map_err(|error| {
        error!(?error, task_id = %claim.task_id, "failed to project claim");
        ThreadplaneServerError::internal(error)
    })
}

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

pub(crate) async fn release_task(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<ReleaseTaskRequest>,
) -> AppResult<TaskRecord> {
    require_workspace_editor()
        .pool(state.pool())
        .bootstrap(state.bootstrap())
        .workspace(&request.workspace)
        .actor(&request.actor)
        .call()
        .await?;
    let mut tx = state.pool().begin().await?;
    let released_at = Utc::now();
    let payload = serde_json::to_value(&request)?;
    let pending_receipt = match begin_idempotent_command::<TaskRecord>(
        &mut tx,
        IdempotencyContext {
            actor: &request.actor,
            command_kind: "release_task",
            idempotency_key: idempotency_key(&headers)?,
            request_payload: &payload,
            workspace: &request.workspace,
        },
        released_at,
    )
    .await?
    {
        CommandExecution::Execute(pending_receipt) => pending_receipt,
        CommandExecution::Replay(envelope) => {
            tx.commit().await?;
            return Ok(Json(envelope));
        }
    };
    fetch_task_by_id_tx(&mut tx, request.task_id, &request.workspace).await?;
    let claim = fetch_active_claim_tx(&mut tx, request.task_id)
        .await?
        .ok_or_else(|| {
            ThreadplaneServerError::conflict("task does not have an active claim to release")
        })?;

    if claim.actor != request.actor {
        return Err(ThreadplaneServerError::conflict(format!(
            "task is currently claimed by {}",
            claim.actor
        )));
    }
    let event_id = append_event(
        &mut tx,
        &request.workspace,
        &request.actor,
        EventKind::TaskReleased,
        &payload,
        released_at,
    )
    .await?;

    sqlx::query(
        "
        UPDATE task_claims
        SET released_at = $2
        WHERE claim_id = $1
        ",
    )
    .bind(claim.claim_id)
    .bind(released_at)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "
        UPDATE tasks
        SET status = 'open',
            updated_at = $2
        WHERE task_id = $1
        ",
    )
    .bind(request.task_id)
    .bind(released_at)
    .execute(&mut *tx)
    .await?;
    let record =
        TaskRecord::from(fetch_task_by_id_tx(&mut tx, request.task_id, &request.workspace).await?);
    let receipt =
        complete_idempotent_command(&mut tx, pending_receipt.as_ref(), &record, released_at)
            .await?;
    tx.commit().await?;
    project_graph_event()
        .pool(state.pool())
        .projection_coordinator(state.projection_coordinator())
        .workspace(&request.workspace)
        .event_id(event_id)
        .operation(Box::pin(mutations::project_task_record(
            state.graph(),
            state.pool(),
            &record,
        )))
        .call()
        .await?;

    Ok(success_with_receipt(record, receipt))
}

pub(crate) async fn complete_task(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CompleteTaskRequest>,
) -> AppResult<TaskRecord> {
    require_workspace_editor()
        .pool(state.pool())
        .bootstrap(state.bootstrap())
        .workspace(&request.workspace)
        .actor(&request.actor)
        .call()
        .await?;
    let mut tx = state.pool().begin().await?;
    let completed_at = Utc::now();
    let payload = serde_json::to_value(&request)?;
    let pending_receipt = match begin_idempotent_command::<TaskRecord>(
        &mut tx,
        IdempotencyContext {
            actor: &request.actor,
            command_kind: "complete_task",
            idempotency_key: idempotency_key(&headers)?,
            request_payload: &payload,
            workspace: &request.workspace,
        },
        completed_at,
    )
    .await?
    {
        CommandExecution::Execute(pending_receipt) => pending_receipt,
        CommandExecution::Replay(envelope) => {
            tx.commit().await?;
            return Ok(Json(envelope));
        }
    };

    if let Some(claim) = fetch_active_claim_tx(&mut tx, request.task_id).await? {
        if claim.actor != request.actor {
            return Err(ThreadplaneServerError::conflict(format!(
                "task is currently claimed by {}",
                claim.actor
            )));
        }
    }
    let event_id = append_event(
        &mut tx,
        &request.workspace,
        &request.actor,
        EventKind::TaskCompleted,
        &payload,
        completed_at,
    )
    .await?;

    sqlx::query(
        "
        UPDATE task_claims
        SET released_at = $2
        WHERE task_id = $1
          AND released_at IS NULL
          AND expires_at > now()
        ",
    )
    .bind(request.task_id)
    .bind(completed_at)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "
        UPDATE tasks
        SET status = 'completed',
            updated_at = $2
        WHERE task_id = $1
        ",
    )
    .bind(request.task_id)
    .bind(completed_at)
    .execute(&mut *tx)
    .await?;
    let record =
        TaskRecord::from(fetch_task_by_id_tx(&mut tx, request.task_id, &request.workspace).await?);
    let receipt =
        complete_idempotent_command(&mut tx, pending_receipt.as_ref(), &record, completed_at)
            .await?;
    tx.commit().await?;
    project_graph_event()
        .pool(state.pool())
        .projection_coordinator(state.projection_coordinator())
        .workspace(&request.workspace)
        .event_id(event_id)
        .operation(Box::pin(mutations::project_task_record(
            state.graph(),
            state.pool(),
            &record,
        )))
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
