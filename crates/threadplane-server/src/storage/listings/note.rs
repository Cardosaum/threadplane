use super::shared::normalized_text_query;
use super::*;

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct NoteListFilters<'filter> {
    pub(crate) author: Option<&'filter str>,
    pub(crate) query: Option<&'filter str>,
}

pub(crate) async fn fetch_notes_for_listing(
    pool: &PgPool,
    workspace: &str,
    filters: NoteListFilters<'_>,
    limit: i64,
) -> ServerResult<Vec<NoteRow>> {
    let normalized_author = normalize_task_owner(filters.author.map(str::to_owned));
    let normalized_query = normalized_text_query(filters.query);
    let mut query = QueryBuilder::<Postgres>::new(NOTE_SELECT);
    query.push(" WHERE workspace = ");
    query.push_bind(workspace);

    if let Some(author) = normalized_author {
        query.push(" AND author = ");
        query.push_bind(author);
    }
    if let Some(search_query) = normalized_query {
        query.push(" AND (title ILIKE ");
        query.push_bind(format!("%{search_query}%"));
        query.push(" OR body ILIKE ");
        query.push_bind(format!("%{search_query}%"));
        query.push(")");
    }
    query.push(" ORDER BY updated_at DESC, created_at DESC");
    query.push(" LIMIT ");
    query.push_bind(limit);

    query
        .build_query_as::<NoteRow>()
        .fetch_all(pool)
        .await
        .map_err(Into::into)
}
