use tokio::time::timeout;
use tracing::{info, warn};

use super::{
    projectors::project_event, AppState, EventRow, GRAPH_PROJECTION_NAME, REPLAY_BATCH_SIZE,
    REPLAY_EVENT_TIMEOUT,
};
use crate::prelude::{ServerResult, ThreadplaneServerError, Utc};
use crate::storage::{
    fetch_event_rows_after_cursor, fetch_projection_cursor, record_projection_cursor,
    ProjectionCursor,
};

pub(crate) async fn catch_up_graph_projection(state: &AppState) -> ServerResult<u64> {
    let mut total_replayed = 0_u64;

    loop {
        let replayed = Box::pin(replay_graph_projection_batch(state, REPLAY_BATCH_SIZE)).await?;
        total_replayed = total_replayed.saturating_add(replayed);

        if replayed == 0 {
            break;
        }
    }

    Ok(total_replayed)
}

pub(super) async fn replay_graph_projection_batch(
    state: &AppState,
    limit: i64,
) -> ServerResult<u64> {
    let events = load_replay_batch(state, limit).await?;
    if events.is_empty() {
        return Ok(0);
    }

    replay_events(state, &events).await?;
    advance_projection_cursor(state, &events).await?;

    Ok(events.len().try_into().unwrap_or(u64::MAX))
}

async fn advance_projection_cursor(state: &AppState, events: &[EventRow]) -> ServerResult<()> {
    let Some(last_event) = events.last() else {
        return Ok(());
    };

    let mut tx = state.pool().begin().await?;
    record_projection_cursor(
        &mut tx,
        GRAPH_PROJECTION_NAME,
        last_event.cursor(),
        Utc::now(),
    )
    .await?;
    tx.commit().await?;

    Ok(())
}

async fn load_replay_batch(state: &AppState, limit: i64) -> ServerResult<Vec<EventRow>> {
    let cursor = load_replay_cursor(state).await?;
    load_replay_events(state, cursor, limit).await
}

async fn load_replay_cursor(state: &AppState) -> ServerResult<Option<ProjectionCursor>> {
    info!(
        projection = GRAPH_PROJECTION_NAME,
        "loading graph projection cursor"
    );
    let cursor = fetch_projection_cursor(state.pool(), GRAPH_PROJECTION_NAME).await?;
    info!(
        projection = GRAPH_PROJECTION_NAME,
        has_cursor = cursor.is_some(),
        "loaded graph projection cursor"
    );
    Ok(cursor)
}

async fn load_replay_events(
    state: &AppState,
    cursor: Option<ProjectionCursor>,
    limit: i64,
) -> ServerResult<Vec<EventRow>> {
    info!(
        projection = GRAPH_PROJECTION_NAME,
        limit, "loading graph projection events"
    );
    let events = fetch_event_rows_after_cursor(state.pool(), cursor, limit).await?;
    info!(
        projection = GRAPH_PROJECTION_NAME,
        events = events.len(),
        "loaded graph projection events"
    );
    Ok(events)
}

async fn replay_events(state: &AppState, events: &[EventRow]) -> ServerResult<()> {
    info!(
        projection = GRAPH_PROJECTION_NAME,
        events = events.len(),
        "replaying graph projection batch"
    );
    for event in events {
        replay_event_with_timeout(state, event).await?;
    }

    Ok(())
}

async fn replay_event_with_timeout(state: &AppState, event: &EventRow) -> ServerResult<()> {
    info!(
        projection = GRAPH_PROJECTION_NAME,
        event_id = %event.event_id,
        kind = %event.kind,
        "replaying graph projection event"
    );
    timeout(REPLAY_EVENT_TIMEOUT, Box::pin(project_event(state, event)))
        .await
        .map_err(|_timeout_error| {
            warn!(
                projection = GRAPH_PROJECTION_NAME,
                event_id = %event.event_id,
                kind = %event.kind,
                timeout_seconds = REPLAY_EVENT_TIMEOUT.as_secs(),
                "graph projection event replay timed out"
            );
            ThreadplaneServerError::internal(format!(
                "graph projection event {} ({}) timed out after {} seconds",
                event.event_id,
                event.kind,
                REPLAY_EVENT_TIMEOUT.as_secs()
            ))
        })?
}
