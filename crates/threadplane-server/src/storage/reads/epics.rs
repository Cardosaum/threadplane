use super::*;

pub(crate) async fn fetch_epic_rows_for_workspace(
    pool: &PgPool,
    workspace: &str,
) -> ServerResult<Vec<EpicRow>> {
    query_as(&format!(
        "
        {EPIC_SELECT}
        WHERE workspace = $1
        ORDER BY created_at DESC
        "
    ))
    .bind(workspace)
    .fetch_all(pool)
    .await
    .map_err(Into::into)
}

pub(crate) async fn fetch_epic_by_id(pool: &PgPool, epic_id: Uuid) -> ServerResult<EpicRow> {
    query_as(&format!("{EPIC_SELECT} WHERE epic_id = $1"))
        .bind(epic_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| ThreadplaneServerError::not_found("epic not found"))
}

pub(crate) async fn fetch_epic_by_event_id(
    pool: &PgPool,
    event_id: Uuid,
) -> ServerResult<Option<EpicRow>> {
    query_as(&format!("{EPIC_SELECT} WHERE event_id = $1"))
        .bind(event_id)
        .fetch_optional(pool)
        .await
        .map_err(Into::into)
}

pub(crate) async fn fetch_epic_by_id_tx(
    tx: &mut Transaction<'_, Postgres>,
    epic_id: Uuid,
    workspace: &str,
) -> ServerResult<EpicRow> {
    query_as(&format!(
        "{EPIC_SELECT} WHERE epic_id = $1 AND workspace = $2"
    ))
    .bind(epic_id)
    .bind(workspace)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| ThreadplaneServerError::not_found("epic not found"))
}

pub(crate) async fn fetch_epic_for_task(
    pool: &PgPool,
    task: &TaskRow,
) -> ServerResult<Option<EpicRecord>> {
    if let Some(epic_id) = task.epic_id {
        return fetch_epic_by_id(pool, epic_id)
            .await
            .map(EpicRecord::from)
            .map(Some);
    }

    Ok(None)
}
