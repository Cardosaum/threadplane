use super::*;

pub(crate) async fn fetch_active_claim(
    pool: &PgPool,
    task_id: Uuid,
) -> ServerResult<Option<ClaimRow>> {
    query_as(
        "
        SELECT claim_id, task_id, workspace, actor, event_id, claimed_at, expires_at, released_at
        FROM task_claims
        WHERE task_id = $1
          AND released_at IS NULL
          AND expires_at > now()
        ORDER BY claimed_at DESC
        LIMIT 1
        ",
    )
    .bind(task_id)
    .fetch_optional(pool)
    .await
    .map_err(Into::into)
}

pub(crate) async fn fetch_claim_by_event_id(
    pool: &PgPool,
    event_id: Uuid,
) -> ServerResult<Option<ClaimRow>> {
    query_as(&format!("{CLAIM_SELECT} WHERE event_id = $1"))
        .bind(event_id)
        .fetch_optional(pool)
        .await
        .map_err(Into::into)
}

pub(crate) async fn fetch_active_claim_tx(
    tx: &mut Transaction<'_, Postgres>,
    task_id: Uuid,
) -> ServerResult<Option<ClaimRow>> {
    query_as(
        "
        SELECT claim_id, task_id, workspace, actor, event_id, claimed_at, expires_at, released_at
        FROM task_claims
        WHERE task_id = $1
          AND released_at IS NULL
          AND expires_at > now()
        ORDER BY claimed_at DESC
        LIMIT 1
        ",
    )
    .bind(task_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(Into::into)
}
