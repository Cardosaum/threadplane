use crate::{
    projections::{
        project_claim, project_epic, project_link, project_memory, project_note, project_task,
        project_task_dependency_by_id, project_task_supporting_entities,
        reproject_transclusion_group,
    },
    storage::{
        fetch_claim_by_event_id, fetch_epic_by_event_id, fetch_link_by_event_id,
        fetch_memory_by_event_id, fetch_note_by_event_id, fetch_note_by_id, fetch_task_by_event_id,
        fetch_task_by_id, fetch_task_dependency_by_event_id,
    },
};
use threadplane_core::{
    ClaimTaskRequest, CompleteTaskRequest, EventKind, ReleaseTaskRequest, TaskRecord,
    UpdateNoteRequest, UpdateTaskRequest,
};
use tracing::warn;

use super::{AppState, EventRow, XanaduReplayPayload};
use crate::prelude::ServerResult;

pub(super) async fn project_event(state: &AppState, event: &EventRow) -> ServerResult<()> {
    Box::pin(state.serialize_graph_projection(async {
        match event.parsed_kind() {
            EventKind::EpicRecorded => project_epic_recorded(state, event).await,
            EventKind::FactPromoted => {
                warn!(event_id = %event.event_id, "fact promotion replay is not implemented yet");
                Ok(())
            }
            EventKind::LinkDeclared => project_link_declared(state, event).await,
            EventKind::MemoryRecorded => project_memory_recorded(state, event).await,
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

async fn project_memory_recorded(state: &AppState, event: &EventRow) -> ServerResult<()> {
    let Some(row) = fetch_memory_by_event_id(state.pool(), event.event_id).await? else {
        warn!(event_id = %event.event_id, "skipping memory replay without matching row");
        return Ok(());
    };

    project_memory(state.graph(), &row.try_into()?).await
}

async fn project_note_updated(state: &AppState, event: &EventRow) -> ServerResult<()> {
    let request: UpdateNoteRequest = serde_json::from_value(event.payload.clone())?;
    let note = fetch_note_by_id(state.pool(), request.note_id).await?;

    if let Some(group_id) = note.transclusion_id {
        return reproject_transclusion_group(state.graph(), state.pool(), group_id, None).await;
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
        return reproject_transclusion_group(state.graph(), state.pool(), group_id, None).await;
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
    reproject_transclusion_group(
        state.graph(),
        state.pool(),
        payload.transclusion_id,
        payload.merged_group_id,
    )
    .await
}

async fn project_task_row(state: &AppState, task: TaskRecord) -> ServerResult<()> {
    project_task_supporting_entities(state.graph(), state.pool(), &task).await?;
    project_task(state.graph(), &task).await
}
