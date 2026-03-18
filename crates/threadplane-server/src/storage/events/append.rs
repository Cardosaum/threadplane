use super::*;

pub(crate) async fn append_event(
    tx: &mut Transaction<'_, Postgres>,
    workspace: &str,
    actor: &str,
    kind: EventKind,
    payload: &Value,
    created_at: DateTime<Utc>,
) -> ServerResult<Uuid> {
    let event_id = Uuid::new_v4();
    sqlx::query(
        "
        INSERT INTO events (event_id, workspace, actor, kind, payload, created_at)
        VALUES ($1, $2, $3, $4, $5, $6)
        ",
    )
    .bind(event_id)
    .bind(workspace)
    .bind(actor)
    .bind(event_kind_name(kind))
    .bind(payload.clone())
    .bind(created_at)
    .execute(&mut **tx)
    .await?;
    Ok(event_id)
}
