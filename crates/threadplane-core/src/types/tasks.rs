#![allow(
    clippy::wildcard_imports,
    reason = "Task submodule reuses shared type imports via the crate-local prelude style"
)]

use super::*;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Display, Serialize, Deserialize)]
#[display("{_0}")]
#[serde(transparent)]
pub struct TaskPriority(String);

impl TaskPriority {
    #[inline]
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[inline]
    #[must_use]
    pub fn from_lossy<T>(value: T) -> Self
    where
        T: Into<String>,
    {
        Self::new(value).unwrap_or_else(|| Self("invalid".to_owned()))
    }

    #[inline]
    #[must_use]
    pub fn new<T>(value: T) -> Option<Self>
    where
        T: Into<String>,
    {
        let normalized = normalize_workspace_priority_name(&value.into());
        (!normalized.is_empty()).then_some(Self(normalized))
    }
}

impl FromStr for TaskPriority {
    type Err = &'static str;

    #[inline]
    fn from_str(input: &str) -> Result<Self, Self::Err> {
        Self::new(input).ok_or("task priority cannot be empty")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskMetadata {
    pub labels: Vec<String>,
    pub owner: Option<String>,
    pub priority: TaskPriority,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRecord {
    pub author: String,
    pub created_at: String,
    pub details: String,
    pub entity_ref: String,
    pub epic_id: Option<Uuid>,
    pub event_id: Uuid,
    #[serde(flatten)]
    pub metadata: TaskMetadata,
    pub status: String,
    pub task_id: Uuid,
    pub title: String,
    pub transclusion_id: Option<Uuid>,
    pub updated_at: String,
    pub workspace: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskClaimRecord {
    pub actor: String,
    pub claim_id: Uuid,
    pub claimed_at: String,
    pub event_id: Uuid,
    pub expires_at: String,
    pub task_id: Uuid,
    pub workspace: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSummary {
    pub author: String,
    pub created_at: String,
    pub details: String,
    pub entity_ref: String,
    pub epic_id: Option<Uuid>,
    #[serde(flatten)]
    pub metadata: TaskMetadata,
    pub status: String,
    pub task_id: Uuid,
    pub title: String,
    pub transclusion_id: Option<Uuid>,
    pub updated_at: String,
    pub workspace: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskDependencySummary {
    pub depth: i32,
    pub entity_ref: String,
    pub status: String,
    pub task_id: Uuid,
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskListEntry {
    pub active_claim: Option<TaskClaimRecord>,
    pub dependencies: Vec<TaskDependencySummary>,
    pub dependents: Vec<TaskDependencySummary>,
    pub epic: Option<EpicRecord>,
    pub ready: bool,
    pub task: TaskSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskDag {
    pub dependencies: Vec<TaskDependencySummary>,
    pub dependents: Vec<TaskDependencySummary>,
    pub epic: Option<EpicRecord>,
    pub ready: bool,
    pub task: TaskSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskContext {
    pub active_claim: Option<TaskClaimRecord>,
    pub dependencies: Vec<TaskDependencySummary>,
    pub dependents: Vec<TaskDependencySummary>,
    pub epic: Option<EpicRecord>,
    pub ready: bool,
    pub relations: Vec<GraphRelation>,
    pub task: TaskSummary,
}

#[inline]
#[must_use]
pub fn normalize_task_labels(labels: Vec<String>) -> Vec<String> {
    let mut normalized = labels
        .into_iter()
        .map(|label| label.trim().to_ascii_lowercase())
        .filter(|label| !label.is_empty())
        .collect::<Vec<_>>();
    normalized.sort_unstable();
    normalized.dedup();
    normalized
}

#[inline]
#[must_use]
pub fn normalize_task_owner(owner: Option<String>) -> Option<String> {
    let value = owner?;
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}
