use super::*;

pub(crate) async fn fetch_projection_status(
    pool: &PgPool,
    projection_name: &str,
) -> ServerResult<ProjectionStatus> {
    let cursor = fetch_projection_cursor(pool, projection_name).await?;
    let total_events = count_all_events(pool).await?;
    let pending_events = count_events_after_cursor(pool, cursor).await?;

    Ok(build_projection_status(
        projection_name,
        cursor,
        total_events,
        pending_events,
    ))
}

async fn count_all_events(pool: &PgPool) -> ServerResult<i64> {
    let (count,): (i64,) = query_as("SELECT COUNT(*) FROM events")
        .fetch_one(pool)
        .await?;
    Ok(count)
}

async fn count_events_after_cursor(
    pool: &PgPool,
    cursor: Option<ProjectionCursor>,
) -> ServerResult<i64> {
    if let Some(current_cursor) = cursor {
        let (count,): (i64,) = query_as(
            "
            SELECT COUNT(*)
            FROM events
            WHERE created_at > $1
               OR (created_at = $1 AND event_id > $2)
            ",
        )
        .bind(current_cursor.created_at)
        .bind(current_cursor.event_id)
        .fetch_one(pool)
        .await?;
        return Ok(count);
    }

    count_all_events(pool).await
}

pub(crate) fn build_projection_status(
    projection_name: &str,
    cursor: Option<ProjectionCursor>,
    total_events: i64,
    pending_events: i64,
) -> ProjectionStatus {
    let projected_events = total_events.saturating_sub(pending_events);
    let (last_event_created_at, last_event_id) = cursor.map_or((None, None), |current_cursor| {
        (
            Some(current_cursor.created_at.to_rfc3339()),
            Some(current_cursor.event_id),
        )
    });

    ProjectionStatus {
        caught_up: pending_events == 0,
        last_event_created_at,
        last_event_id,
        pending_events,
        projected_events,
        projection_name: projection_name.to_owned(),
        total_events,
    }
}
