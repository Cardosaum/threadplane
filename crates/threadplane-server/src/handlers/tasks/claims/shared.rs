use super::*;

pub(super) async fn ensure_task_is_unclaimed(
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
pub(super) async fn persist_task_claim(
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
pub(super) async fn project_claimed_task_record(
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
