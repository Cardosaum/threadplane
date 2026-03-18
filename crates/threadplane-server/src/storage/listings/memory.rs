use super::shared::{
    normalized_memory_recall_trigger_filter, normalized_memory_tag_filter, normalized_text_query,
};
use super::*;

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct MemoryListFilters<'filter> {
    pub(crate) audience: Option<MemoryAudience>,
    pub(crate) importance: Option<MemoryImportance>,
    pub(crate) kind: Option<&'filter MemoryKind>,
    pub(crate) query: Option<&'filter str>,
    pub(crate) recall_trigger: Option<&'filter str>,
    pub(crate) tag: Option<&'filter str>,
}

pub(crate) async fn fetch_memories_for_listing(
    pool: &PgPool,
    workspace: &str,
    filters: MemoryListFilters<'_>,
    limit: i64,
) -> ServerResult<Vec<MemoryRow>> {
    let normalized_query = normalized_text_query(filters.query);
    let normalized_tag = normalized_memory_tag_filter(filters.tag);
    let normalized_recall_trigger = normalized_memory_recall_trigger_filter(filters.recall_trigger);

    let mut query = QueryBuilder::<Postgres>::new(MEMORY_SELECT);
    query.push(" WHERE workspace = ");
    query.push_bind(workspace);

    if let Some(kind) = filters.kind {
        query.push(" AND kind = ");
        query.push_bind(kind.as_str());
    }
    if let Some(audience) = filters.audience {
        query.push(" AND audience IN (");
        query.push_bind(audience.to_string());
        query.push(", ");
        query.push_bind(MemoryAudience::Both.to_string());
        query.push(")");
    }
    if let Some(importance) = filters.importance {
        query.push(" AND importance = ");
        query.push_bind(importance.to_string());
    }
    if let Some(tag) = normalized_tag {
        query.push(" AND tags @> ");
        query.push_bind(vec![tag]);
        query.push("::text[]");
    }
    if let Some(recall_trigger) = normalized_recall_trigger {
        query.push(" AND recall_triggers @> ");
        query.push_bind(vec![recall_trigger]);
        query.push("::text[]");
    }
    if let Some(search_query) = normalized_query {
        query.push(" AND (title ILIKE ");
        query.push_bind(format!("%{search_query}%"));
        query.push(" OR body ILIKE ");
        query.push_bind(format!("%{search_query}%"));
        query.push(")");
    }
    query.push(
        " ORDER BY CASE importance \
            WHEN 'critical' THEN 30 \
            WHEN 'high' THEN 20 \
            ELSE 10 \
          END DESC, updated_at DESC, created_at DESC",
    );
    query.push(" LIMIT ");
    query.push_bind(limit);

    query
        .build_query_as::<MemoryRow>()
        .fetch_all(pool)
        .await
        .map_err(Into::into)
}
