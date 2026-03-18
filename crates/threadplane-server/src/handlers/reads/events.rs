#![allow(
    clippy::wildcard_imports,
    reason = "Read handlers use the parent handler module as their import boundary."
)]

use super::super::*;

pub(crate) async fn list_events(
    State(pool): State<PgPool>,
    Path(WorkspacePath { workspace }): Path<WorkspacePath>,
    Query(query): Query<ListQuery>,
) -> AppResult<Vec<EventRecord>> {
    let rows =
        fetch_event_rows_for_workspace(&pool, &workspace, normalized_list_limit(query.limit))
            .await?;
    Ok(success(rows.into_iter().map(EventRecord::from).collect()))
}

pub(crate) async fn tail_events(
    State(pool): State<PgPool>,
    Path(WorkspacePath { workspace }): Path<WorkspacePath>,
    Query(query): Query<EventTailQuery>,
) -> AppResult<Vec<EventRecord>> {
    let cursor = if let Some(event_id) = query.after_event_id {
        let event = fetch_event_row_for_workspace(&pool, &workspace, event_id).await?;
        Some(event.cursor())
    } else {
        None
    };
    let rows = fetch_event_rows_after_workspace_cursor(
        &pool,
        &workspace,
        cursor,
        normalized_list_limit(query.limit),
    )
    .await?;
    Ok(success(rows.into_iter().map(EventRecord::from).collect()))
}
