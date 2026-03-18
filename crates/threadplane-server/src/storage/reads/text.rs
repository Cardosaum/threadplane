use super::*;

pub(crate) async fn fetch_note_by_id(pool: &PgPool, note_id: Uuid) -> ServerResult<NoteRow> {
    query_as(&format!("{NOTE_SELECT} WHERE note_id = $1"))
        .bind(note_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| ThreadplaneServerError::not_found("note not found"))
}

pub(crate) async fn fetch_memory_by_id(pool: &PgPool, memory_id: Uuid) -> ServerResult<MemoryRow> {
    query_as(&format!("{MEMORY_SELECT} WHERE memory_id = $1"))
        .bind(memory_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| ThreadplaneServerError::not_found("memory not found"))
}

pub(crate) async fn fetch_memory_by_event_id(
    pool: &PgPool,
    event_id: Uuid,
) -> ServerResult<Option<MemoryRow>> {
    query_as(&format!("{MEMORY_SELECT} WHERE event_id = $1"))
        .bind(event_id)
        .fetch_optional(pool)
        .await
        .map_err(Into::into)
}

pub(crate) async fn fetch_memory_by_id_tx(
    tx: &mut Transaction<'_, Postgres>,
    memory_id: Uuid,
    workspace: &str,
) -> ServerResult<MemoryRow> {
    query_as(&format!(
        "{MEMORY_SELECT} WHERE memory_id = $1 AND workspace = $2"
    ))
    .bind(memory_id)
    .bind(workspace)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| ThreadplaneServerError::not_found("memory not found"))
}

pub(crate) async fn fetch_note_by_event_id(
    pool: &PgPool,
    event_id: Uuid,
) -> ServerResult<Option<NoteRow>> {
    query_as(&format!("{NOTE_SELECT} WHERE event_id = $1"))
        .bind(event_id)
        .fetch_optional(pool)
        .await
        .map_err(Into::into)
}

pub(crate) async fn fetch_note_by_id_tx(
    tx: &mut Transaction<'_, Postgres>,
    note_id: Uuid,
    workspace: &str,
) -> ServerResult<NoteRow> {
    query_as(&format!(
        "{NOTE_SELECT} WHERE note_id = $1 AND workspace = $2"
    ))
    .bind(note_id)
    .bind(workspace)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| ThreadplaneServerError::not_found("note not found"))
}

pub(crate) async fn fetch_link_by_event_id(
    pool: &PgPool,
    event_id: Uuid,
) -> ServerResult<Option<LinkRow>> {
    query_as(&format!("{LINK_SELECT} WHERE event_id = $1"))
        .bind(event_id)
        .fetch_optional(pool)
        .await
        .map_err(Into::into)
}
