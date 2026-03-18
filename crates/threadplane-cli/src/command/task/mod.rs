#![allow(
    clippy::wildcard_imports,
    reason = "Task command definitions intentionally build on the command module prelude."
)]

mod commands;
mod helpers;
mod metadata;
mod status;
mod triage;

pub(crate) use self::commands::{
    AddTaskDependency, ClaimNextTask, ClaimTask, CompleteTask, ListTasks, NextTask, OfferTask,
    ReleaseTask, ShowTask, TaskCommand, TaskContextCommand, TaskDagCommand,
    TaskDependencyViewCommand, TaskSubcommand, TriageTasks, UpdateTask,
};
pub(crate) use self::helpers::{
    apply_metadata_patch, normalize_priority_name, parse_task_priority_input,
    select_dependency_view_from_context, select_dependency_view_from_dag, task_metadata_changed,
    task_metadata_from_args, triage_has_changes, triage_summary_labels, triage_summary_owner,
};
pub(crate) use self::metadata::{TaskMetadataArgs, TaskMetadataFilterArgs, TaskMetadataPatchArgs};
pub(crate) use self::status::TaskStatusValue;
pub(crate) use self::triage::{
    dedup_task_ids, TaskDependencyViewKind, TaskTriageOutcome, TaskTriageSummary,
};
