use super::*;

pub(crate) async fn fetch_task_by_id(pool: &PgPool, task_id: Uuid) -> ServerResult<TaskRow> {
    query_as(&format!("{TASK_SELECT} WHERE task_id = $1"))
        .bind(task_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| ThreadplaneServerError::not_found("task not found"))
}

pub(crate) async fn fetch_task_by_event_id(
    pool: &PgPool,
    event_id: Uuid,
) -> ServerResult<Option<TaskRow>> {
    query_as(&format!("{TASK_SELECT} WHERE event_id = $1"))
        .bind(event_id)
        .fetch_optional(pool)
        .await
        .map_err(Into::into)
}

pub(crate) async fn fetch_task_by_id_tx(
    tx: &mut Transaction<'_, Postgres>,
    task_id: Uuid,
    workspace: &str,
) -> ServerResult<TaskRow> {
    query_as(&format!(
        "{TASK_SELECT} WHERE task_id = $1 AND workspace = $2"
    ))
    .bind(task_id)
    .bind(workspace)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| ThreadplaneServerError::not_found("task not found"))
}
