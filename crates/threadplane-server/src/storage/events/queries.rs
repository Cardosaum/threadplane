use super::*;

pub(crate) async fn fetch_event_rows_for_workspace(
    pool: &PgPool,
    workspace: &str,
    limit: i64,
) -> ServerResult<Vec<EventRow>> {
    query_as(
        "
        SELECT event_id, workspace, actor, kind, payload, created_at
        FROM events
        WHERE workspace = $1
        ORDER BY created_at DESC
        LIMIT $2
        ",
    )
    .bind(workspace)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(Into::into)
}

pub(crate) async fn fetch_event_row_for_workspace(
    pool: &PgPool,
    workspace: &str,
    event_id: Uuid,
) -> ServerResult<EventRow> {
    query_as(
        "
        SELECT event_id, workspace, actor, kind, payload, created_at
        FROM events
        WHERE workspace = $1
          AND event_id = $2
        ",
    )
    .bind(workspace)
    .bind(event_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| ThreadplaneServerError::not_found("event not found"))
}

pub(crate) async fn fetch_event_rows_after_workspace_cursor(
    pool: &PgPool,
    workspace: &str,
    cursor: Option<ProjectionCursor>,
    limit: i64,
) -> ServerResult<Vec<EventRow>> {
    if let Some(current_cursor) = cursor {
        return query_as(
            "
            SELECT event_id, workspace, actor, kind, payload, created_at
            FROM events
            WHERE workspace = $1
              AND (
                    created_at > $2
                 OR (created_at = $2 AND event_id > $3)
              )
            ORDER BY created_at ASC, event_id ASC
            LIMIT $4
            ",
        )
        .bind(workspace)
        .bind(current_cursor.created_at)
        .bind(current_cursor.event_id)
        .bind(limit)
        .fetch_all(pool)
        .await
        .map_err(Into::into);
    }

    let mut rows: Vec<EventRow> = query_as(
        "
        SELECT event_id, workspace, actor, kind, payload, created_at
        FROM events
        WHERE workspace = $1
        ORDER BY created_at DESC, event_id DESC
        LIMIT $2
        ",
    )
    .bind(workspace)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    rows.reverse();
    Ok(rows)
}

pub(crate) async fn fetch_event_rows_after_cursor(
    pool: &PgPool,
    cursor: Option<ProjectionCursor>,
    limit: i64,
) -> ServerResult<Vec<EventRow>> {
    if let Some(current_cursor) = cursor {
        return query_as(
            "
            SELECT event_id, workspace, actor, kind, payload, created_at
            FROM events
            WHERE created_at > $1
               OR (created_at = $1 AND event_id > $2)
            ORDER BY created_at ASC, event_id ASC
            LIMIT $3
            ",
        )
        .bind(current_cursor.created_at)
        .bind(current_cursor.event_id)
        .bind(limit)
        .fetch_all(pool)
        .await
        .map_err(Into::into);
    }

    query_as(
        "
        SELECT event_id, workspace, actor, kind, payload, created_at
        FROM events
        ORDER BY created_at ASC, event_id ASC
        LIMIT $1
        ",
    )
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(Into::into)
}
