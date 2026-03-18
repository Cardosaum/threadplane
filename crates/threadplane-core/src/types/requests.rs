#![allow(
    clippy::wildcard_imports,
    reason = "The request type submodule intentionally builds on the shared types prelude."
)]

use super::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateNoteRequest {
    pub author: String,
    pub body: String,
    pub title: String,
    pub workspace: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateMemoryRequest {
    pub audience: MemoryAudience,
    pub author: String,
    pub body: String,
    pub importance: MemoryImportance,
    pub kind: MemoryKind,
    pub recall_triggers: Vec<String>,
    pub scope: MemoryScope,
    pub tags: Vec<String>,
    pub title: String,
    pub workspace: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateNoteRequest {
    pub actor: String,
    pub body: String,
    pub note_id: Uuid,
    pub title: String,
    pub workspace: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OfferTaskRequest {
    pub author: String,
    pub depends_on: Vec<Uuid>,
    pub details: String,
    pub epic_id: Option<Uuid>,
    #[serde(flatten)]
    pub metadata: TaskMetadata,
    pub title: String,
    pub workspace: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateTaskRequest {
    pub actor: String,
    pub details: String,
    pub epic_id: Option<Uuid>,
    #[serde(flatten)]
    pub metadata: TaskMetadata,
    pub task_id: Uuid,
    pub title: String,
    pub workspace: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateEpicRequest {
    pub author: String,
    pub body: String,
    pub title: String,
    pub workspace: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateWorkspacePolicyRequest {
    pub actor: String,
    pub auth: WorkspaceAuthPolicy,
    pub priorities: WorkspacePriorityPolicy,
    pub workspace: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrantWorkspaceMembershipRequest {
    pub actor: String,
    pub member_actor_id: String,
    pub role: WorkspaceRole,
    pub workspace: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddWorkspacePublicKeyRequest {
    pub actor: String,
    pub algorithm: PublicKeyAlgorithm,
    pub key_id: String,
    pub member_actor_id: String,
    pub public_key: String,
    pub workspace: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaimTaskRequest {
    pub actor: String,
    pub lease_seconds: Option<i64>,
    pub task_id: Uuid,
    pub workspace: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaimNextTaskRequest {
    pub actor: String,
    pub epic_id: Option<Uuid>,
    pub label: Option<String>,
    pub lease_seconds: Option<i64>,
    pub owner: Option<String>,
    pub priority: Option<TaskPriority>,
    pub workspace: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseTaskRequest {
    pub actor: String,
    pub task_id: Uuid,
    pub workspace: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompleteTaskRequest {
    pub actor: String,
    pub task_id: Uuid,
    pub workspace: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddTaskDependencyRequest {
    pub actor: String,
    pub depends_on_task_id: Uuid,
    pub task_id: Uuid,
    pub workspace: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddLinkRequest {
    pub actor: String,
    pub from: String,
    pub relation: String,
    pub to: String,
    pub workspace: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateXanaduLinkRequest {
    pub actor: String,
    pub from: String,
    pub to: String,
    pub workspace: String,
}
