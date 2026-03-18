use super::super::*;
use alloc::collections::BTreeSet;

#[derive(Debug, Serialize)]
pub(crate) struct TaskTriageSummary {
    pub(crate) clear_labels: bool,
    pub(crate) clear_owner: bool,
    pub(crate) completed_task_ids: Vec<Uuid>,
    pub(crate) epic_id: Option<Uuid>,
    pub(crate) labels: Option<Vec<String>>,
    pub(crate) owner: Option<String>,
    pub(crate) priority: Option<TaskPriority>,
    pub(crate) task_ids: Vec<Uuid>,
    pub(crate) unchanged_task_ids: Vec<Uuid>,
    pub(crate) updated_task_ids: Vec<Uuid>,
    pub(crate) workspace: String,
}

#[derive(Debug, Default)]
pub(crate) struct TaskTriageOutcome {
    pub(crate) changed: bool,
    pub(crate) completed: bool,
    pub(crate) updated: bool,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum TaskDependencyViewKind {
    BlockedBy,
    Blocks,
}

pub(crate) fn dedup_task_ids(task_ids: &[Uuid]) -> Vec<Uuid> {
    task_ids
        .iter()
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}
