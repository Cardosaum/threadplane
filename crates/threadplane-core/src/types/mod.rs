mod build;
mod governance;
mod records;
mod requests;
mod tasks;

use core::str::FromStr;
use derive_more::Display;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use strum::IntoEnumIterator as _;
use uuid::Uuid;

pub use build::{
    build_info, compare_build_info, health_summary, scope_summary, service_snapshot,
    BuildComparison, BuildFieldDifference, BuildInfo, CommandReceipt, ProjectionStatus,
    ServiceSnapshot,
};
pub use governance::{
    normalize_workspace_priority_name, validate_workspace_auth_policy, validate_workspace_policy,
    validate_workspace_priority_policy, ActorPublicKey, PublicKeyAlgorithm, WorkspaceAuthPolicy,
    WorkspaceMembership, WorkspacePolicy, WorkspacePolicyValidationError, WorkspacePriority,
    WorkspacePriorityPolicy, WorkspaceRole,
};
pub use records::{
    normalize_memory_kind_name, normalize_memory_recall_triggers, normalize_memory_tags,
    EpicRecord, EventRecord, LinkRecord, MemoryAudience, MemoryImportance, MemoryKind,
    MemoryRecord, MemoryScope, NoteRecord,
};
pub use requests::{
    AddLinkRequest, AddTaskDependencyRequest, AddWorkspacePublicKeyRequest, ClaimNextTaskRequest,
    ClaimTaskRequest, CompleteTaskRequest, CreateEpicRequest, CreateMemoryRequest,
    CreateNoteRequest, CreateXanaduLinkRequest, GrantWorkspaceMembershipRequest, OfferTaskRequest,
    ReleaseTaskRequest, UpdateNoteRequest, UpdateTaskRequest, UpdateWorkspacePolicyRequest,
};
pub use tasks::{
    normalize_task_labels, normalize_task_owner, TaskClaimRecord, TaskContext, TaskDag,
    TaskDependencySummary, TaskListEntry, TaskMetadata, TaskPriority, TaskRecord, TaskSummary,
};

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    strum::Display,
    strum::EnumIter,
    strum::EnumString,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum EventKind {
    EpicRecorded,
    FactPromoted,
    LinkDeclared,
    MemoryRecorded,
    NoteRecorded,
    NoteUpdated,
    TaskClaimed,
    TaskCompleted,
    TaskDependencyDeclared,
    TaskOffered,
    TaskReleased,
    TaskUpdated,
    XanaduLinked,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventEnvelope {
    pub actor: String,
    pub kind: EventKind,
    pub payload: Value,
    pub workspace: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct GraphRelation {
    pub body: Option<String>,
    pub direction: String,
    pub entity_kind: String,
    pub entity_ref: String,
    pub relation: String,
    pub title: Option<String>,
    pub transclusion_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "record", rename_all = "snake_case")]
pub enum EntityRecord {
    Epic(EpicRecord),
    Memory(MemoryRecord),
    Note(NoteRecord),
    Task(TaskRecord),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityContext {
    pub entity: EntityRecord,
    pub relations: Vec<GraphRelation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiEnvelope<T> {
    pub data: T,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub receipt: Option<CommandReceipt>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Display)]
pub enum EntityRef {
    #[display("epic:{_0}")]
    Epic(Uuid),
    #[display("memory:{_0}")]
    Memory(Uuid),
    #[display("note:{_0}")]
    Note(Uuid),
    #[display("task:{_0}")]
    Task(Uuid),
}

#[inline]
#[must_use]
pub fn task_entity_ref(task_id: Uuid) -> String {
    EntityRef::Task(task_id).to_string()
}

#[inline]
#[must_use]
pub fn note_entity_ref(note_id: Uuid) -> String {
    EntityRef::Note(note_id).to_string()
}

#[inline]
#[must_use]
pub fn memory_entity_ref(memory_id: Uuid) -> String {
    EntityRef::Memory(memory_id).to_string()
}

#[inline]
#[must_use]
pub fn epic_entity_ref(epic_id: Uuid) -> String {
    EntityRef::Epic(epic_id).to_string()
}

#[inline]
#[must_use]
pub fn parse_entity_ref(input: &str) -> Option<EntityRef> {
    let (kind, raw_id) = input.split_once(':')?;
    let id = Uuid::parse_str(raw_id).ok()?;
    match kind {
        "epic" => Some(EntityRef::Epic(id)),
        "memory" => Some(EntityRef::Memory(id)),
        "note" => Some(EntityRef::Note(id)),
        "task" => Some(EntityRef::Task(id)),
        _ => None,
    }
}

#[inline]
#[must_use]
pub fn relation_type(input: &str) -> String {
    let mut last_was_underscore = false;
    let mut relation = String::new();

    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            relation.push(ch.to_ascii_uppercase());
            last_was_underscore = false;
            continue;
        }

        if !last_was_underscore {
            relation.push('_');
            last_was_underscore = true;
        }
    }

    relation.trim_matches('_').to_owned()
}

fn normalize_identifier(input: &str) -> String {
    relation_type(input).to_ascii_lowercase()
}
