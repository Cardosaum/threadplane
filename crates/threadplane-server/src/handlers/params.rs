use serde::Deserialize;
use uuid::Uuid;

use threadplane_core::{MemoryAudience, MemoryImportance, MemoryKind, TaskPriority};

#[derive(Debug, Deserialize)]
pub(crate) struct EntityPath {
    pub(crate) entity_ref: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct EpicPath {
    pub(crate) epic_id: Uuid,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ListQuery {
    pub(crate) limit: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct MemoryPath {
    pub(crate) memory_id: Uuid,
}

#[derive(Debug, Deserialize)]
pub(crate) struct EventTailQuery {
    pub(crate) after_event_id: Option<Uuid>,
    pub(crate) limit: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct NoteListQuery {
    pub(crate) author: Option<String>,
    pub(crate) limit: Option<i64>,
    pub(crate) query: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct NotePath {
    pub(crate) note_id: Uuid,
}

#[derive(Debug, Deserialize)]
pub(crate) struct MemoryListQuery {
    pub(crate) audience: Option<MemoryAudience>,
    pub(crate) importance: Option<MemoryImportance>,
    pub(crate) kind: Option<MemoryKind>,
    pub(crate) limit: Option<i64>,
    pub(crate) query: Option<String>,
    pub(crate) recall_trigger: Option<String>,
    pub(crate) tag: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct TaskListQuery {
    pub(crate) epic_id: Option<Uuid>,
    pub(crate) label: Option<String>,
    pub(crate) limit: Option<i64>,
    pub(crate) owner: Option<String>,
    pub(crate) priority: Option<TaskPriority>,
    pub(crate) ready_only: Option<bool>,
    pub(crate) status: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct TaskPath {
    pub(crate) task_id: Uuid,
}

#[derive(Debug, Deserialize)]
pub(crate) struct WorkspaceKeysQuery {
    pub(crate) actor_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct WorkspacePath {
    pub(crate) workspace: String,
}
