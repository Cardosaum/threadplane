#![allow(
    clippy::wildcard_imports,
    reason = "Read handlers use the parent handler module as their import boundary."
)]

use super::super::*;

fn memory_list_filters(query: &MemoryListQuery) -> MemoryListFilters<'_> {
    MemoryListFilters {
        audience: query.audience,
        importance: query.importance,
        kind: query.kind.as_ref(),
        query: query.query.as_deref(),
        recall_trigger: query.recall_trigger.as_deref(),
        tag: query.tag.as_deref(),
    }
}

async fn list_memory_records(
    pool: &PgPool,
    workspace: &str,
    filters: MemoryListFilters<'_>,
    limit: i64,
) -> ServerResult<Vec<MemoryRecord>> {
    fetch_memories_for_listing(pool, workspace, filters, limit)
        .await?
        .into_iter()
        .map(MemoryRecord::try_from)
        .collect()
}

pub(crate) async fn show_epic(
    State(pool): State<PgPool>,
    Path(EpicPath { epic_id }): Path<EpicPath>,
) -> AppResult<EpicRecord> {
    let row = fetch_epic_by_id(&pool, epic_id).await?;
    Ok(success(EpicRecord::from(row)))
}

pub(crate) async fn show_memory(
    State(pool): State<PgPool>,
    Path(MemoryPath { memory_id }): Path<MemoryPath>,
) -> AppResult<MemoryRecord> {
    let row = fetch_memory_by_id(&pool, memory_id).await?;
    Ok(success(MemoryRecord::try_from(row)?))
}

pub(crate) async fn list_memories(
    State(bootstrap): State<WorkspaceGovernanceBootstrap>,
    State(pool): State<PgPool>,
    Path(WorkspacePath { workspace }): Path<WorkspacePath>,
    Query(query): Query<MemoryListQuery>,
) -> AppResult<Vec<MemoryRecord>> {
    ensure_workspace_policy(&pool, &bootstrap, &workspace).await?;
    let data = list_memory_records(
        &pool,
        &workspace,
        memory_list_filters(&query),
        normalized_list_limit(query.limit),
    )
    .await?;
    Ok(success(data))
}

pub(crate) async fn prime_memories(
    State(bootstrap): State<WorkspaceGovernanceBootstrap>,
    State(pool): State<PgPool>,
    Path(WorkspacePath { workspace }): Path<WorkspacePath>,
    Query(query): Query<MemoryListQuery>,
) -> AppResult<Vec<MemoryRecord>> {
    ensure_workspace_policy(&pool, &bootstrap, &workspace).await?;
    let data = list_memory_records(
        &pool,
        &workspace,
        MemoryListFilters {
            audience: Some(query.audience.unwrap_or(MemoryAudience::Agent)),
            recall_trigger: query.recall_trigger.as_deref().or(Some("session_start")),
            tag: query.tag.as_deref().or(Some("prime")),
            ..memory_list_filters(&query)
        },
        normalized_list_limit(query.limit),
    )
    .await?;
    Ok(success(data))
}

pub(crate) async fn show_note(
    State(pool): State<PgPool>,
    Path(NotePath { note_id }): Path<NotePath>,
) -> AppResult<NoteRecord> {
    let row = fetch_note_by_id(&pool, note_id).await?;
    Ok(success(NoteRecord::from(row)))
}

pub(crate) async fn list_notes(
    State(bootstrap): State<WorkspaceGovernanceBootstrap>,
    State(pool): State<PgPool>,
    Path(WorkspacePath { workspace }): Path<WorkspacePath>,
    Query(query): Query<NoteListQuery>,
) -> AppResult<Vec<NoteRecord>> {
    ensure_workspace_policy(&pool, &bootstrap, &workspace).await?;
    let rows = fetch_notes_for_listing(
        &pool,
        &workspace,
        NoteListFilters {
            author: query.author.as_deref(),
            query: query.query.as_deref(),
        },
        normalized_list_limit(query.limit),
    )
    .await?;
    Ok(success(rows.into_iter().map(NoteRecord::from).collect()))
}

pub(crate) async fn list_epics(
    State(bootstrap): State<WorkspaceGovernanceBootstrap>,
    State(pool): State<PgPool>,
    Path(WorkspacePath { workspace }): Path<WorkspacePath>,
) -> AppResult<Vec<EpicRecord>> {
    ensure_workspace_policy(&pool, &bootstrap, &workspace).await?;
    let rows = fetch_epic_rows_for_workspace(&pool, &workspace).await?;
    Ok(success(rows.into_iter().map(EpicRecord::from).collect()))
}
