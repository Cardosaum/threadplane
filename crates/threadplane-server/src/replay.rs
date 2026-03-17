#![expect(
    clippy::redundant_pub_crate,
    reason = "Replay helpers are crate-local orchestration around durable projections."
)]

use core::time::Duration;

use chrono::Utc;
use serde::Deserialize;
use tokio::{
    task::JoinHandle,
    time::{sleep, timeout},
};
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::{
    app::AppState,
    error::{ServerResult, ThreadplaneServerError},
    projections::{
        project_claim, project_epic, project_link, project_note, project_task,
        project_task_dependency_by_id, project_task_supporting_entities,
        reproject_transclusion_group,
    },
    storage::{
        fetch_claim_by_event_id, fetch_epic_by_event_id, fetch_event_rows_after_cursor,
        fetch_link_by_event_id, fetch_note_by_event_id, fetch_note_by_id, fetch_projection_cursor,
        fetch_task_by_event_id, fetch_task_by_id, fetch_task_dependency_by_event_id,
        record_projection_cursor, EventRow, ProjectionCursor,
    },
};
use threadplane_core::{
    ClaimTaskRequest, CompleteTaskRequest, EventKind, ReleaseTaskRequest, UpdateNoteRequest,
    UpdateTaskRequest,
};

pub(crate) const GRAPH_PROJECTION_NAME: &str = "neo4j_graph";
const REPLAY_BATCH_SIZE: i64 = 128;
const REPLAY_EVENT_TIMEOUT: Duration = Duration::from_secs(10);
const REPLAY_IDLE_POLL_INTERVAL: Duration = Duration::from_secs(2);

#[derive(Debug, Deserialize)]
struct XanaduReplayPayload {
    merged_group_id: Option<Uuid>,
    transclusion_id: Uuid,
}

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

pub(crate) fn spawn_graph_projection_worker(
    state: AppState,
    shutdown_token: CancellationToken,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            match Box::pin(replay_graph_projection_batch(&state, REPLAY_BATCH_SIZE)).await {
                Ok(0) => {
                    tokio::select! {
                        () = shutdown_token.cancelled() => break,
                        () = sleep(REPLAY_IDLE_POLL_INTERVAL) => {},
                    }
                }
                Ok(replayed) => info!(
                    projection = GRAPH_PROJECTION_NAME,
                    replayed, "replayed graph projection batch"
                ),
                Err(error) => {
                    error!(
                        ?error,
                        projection = GRAPH_PROJECTION_NAME,
                        "graph projection replay failed"
                    );
                    tokio::select! {
                        () = shutdown_token.cancelled() => break,
                        () = sleep(REPLAY_IDLE_POLL_INTERVAL) => {},
                    }
                }
            }
        }
    })
}

async fn replay_graph_projection_batch(state: &AppState, limit: i64) -> ServerResult<u64> {
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

async fn project_event(state: &AppState, event: &EventRow) -> ServerResult<()> {
    Box::pin(state.serialize_graph_projection(async {
        match event.parsed_kind() {
            EventKind::EpicRecorded => project_epic_recorded(state, event).await,
            EventKind::FactPromoted => {
                warn!(event_id = %event.event_id, "fact promotion replay is not implemented yet");
                Ok(())
            }
            EventKind::LinkDeclared => project_link_declared(state, event).await,
            EventKind::NoteRecorded => project_note_recorded(state, event).await,
            EventKind::NoteUpdated => project_note_updated(state, event).await,
            EventKind::TaskClaimed => project_task_claimed(state, event).await,
            EventKind::TaskCompleted => project_task_completed(state, event).await,
            EventKind::TaskDependencyDeclared => {
                project_task_dependency_declared(state, event).await
            }
            EventKind::TaskOffered => project_task_offered(state, event).await,
            EventKind::TaskReleased => project_task_released(state, event).await,
            EventKind::TaskUpdated => project_task_updated(state, event).await,
            EventKind::XanaduLinked => project_xanadu_linked(state, event).await,
        }
    }))
    .await
}

async fn project_epic_recorded(state: &AppState, event: &EventRow) -> ServerResult<()> {
    let Some(row) = fetch_epic_by_event_id(state.pool(), event.event_id).await? else {
        warn!(event_id = %event.event_id, "skipping epic replay without matching row");
        return Ok(());
    };

    project_epic(state.graph(), &row.into()).await
}

async fn project_note_recorded(state: &AppState, event: &EventRow) -> ServerResult<()> {
    let Some(row) = fetch_note_by_event_id(state.pool(), event.event_id).await? else {
        warn!(event_id = %event.event_id, "skipping note replay without matching row");
        return Ok(());
    };

    project_note(state.graph(), &row.into()).await
}

async fn project_note_updated(state: &AppState, event: &EventRow) -> ServerResult<()> {
    let request: UpdateNoteRequest = serde_json::from_value(event.payload.clone())?;
    let note = fetch_note_by_id(state.pool(), request.note_id).await?;

    if let Some(group_id) = note.transclusion_id {
        return reproject_transclusion_group(state, group_id, None).await;
    }

    project_note(state.graph(), &note.into()).await
}

async fn project_task_offered(state: &AppState, event: &EventRow) -> ServerResult<()> {
    let Some(row) = fetch_task_by_event_id(state.pool(), event.event_id).await? else {
        warn!(event_id = %event.event_id, "skipping task replay without matching row");
        return Ok(());
    };

    project_task_row(state, row.into()).await
}

async fn project_task_updated(state: &AppState, event: &EventRow) -> ServerResult<()> {
    let request: UpdateTaskRequest = serde_json::from_value(event.payload.clone())?;
    let task = fetch_task_by_id(state.pool(), request.task_id).await?;

    if let Some(group_id) = task.transclusion_id {
        return reproject_transclusion_group(state, group_id, None).await;
    }

    project_task_row(state, task.into()).await
}

async fn project_task_claimed(state: &AppState, event: &EventRow) -> ServerResult<()> {
    let request: ClaimTaskRequest = serde_json::from_value(event.payload.clone())?;
    let Some(claim) = fetch_claim_by_event_id(state.pool(), event.event_id).await? else {
        warn!(event_id = %event.event_id, "skipping claim replay without matching row");
        return Ok(());
    };

    let task = fetch_task_by_id(state.pool(), request.task_id).await?;
    let task_record = task.clone().into();
    project_task_row(state, task_record).await?;
    project_claim(state.graph(), &task, &claim.into()).await
}

async fn project_task_released(state: &AppState, event: &EventRow) -> ServerResult<()> {
    let request: ReleaseTaskRequest = serde_json::from_value(event.payload.clone())?;
    let task = fetch_task_by_id(state.pool(), request.task_id).await?;
    project_task_row(state, task.into()).await
}

async fn project_task_completed(state: &AppState, event: &EventRow) -> ServerResult<()> {
    let request: CompleteTaskRequest = serde_json::from_value(event.payload.clone())?;
    let task = fetch_task_by_id(state.pool(), request.task_id).await?;
    project_task_row(state, task.into()).await
}

async fn project_task_dependency_declared(state: &AppState, event: &EventRow) -> ServerResult<()> {
    let Some(dependency) = fetch_task_dependency_by_event_id(state.pool(), event.event_id).await?
    else {
        warn!(
            event_id = %event.event_id,
            "skipping dependency replay without matching row"
        );
        return Ok(());
    };

    project_task_dependency_by_id(
        state.graph(),
        state.pool(),
        dependency.task_id,
        dependency.depends_on_task_id,
    )
    .await
}

async fn project_link_declared(state: &AppState, event: &EventRow) -> ServerResult<()> {
    let Some(link) = fetch_link_by_event_id(state.pool(), event.event_id).await? else {
        warn!(event_id = %event.event_id, "skipping link replay without matching row");
        return Ok(());
    };

    project_link(state.graph(), &link.into()).await
}

async fn project_xanadu_linked(state: &AppState, event: &EventRow) -> ServerResult<()> {
    let payload: XanaduReplayPayload = serde_json::from_value(event.payload.clone())?;
    reproject_transclusion_group(state, payload.transclusion_id, payload.merged_group_id).await
}

async fn project_task_row(
    state: &AppState,
    task: threadplane_core::TaskRecord,
) -> ServerResult<()> {
    project_task_supporting_entities(state, &task).await?;
    project_task(state.graph(), &task).await
}
