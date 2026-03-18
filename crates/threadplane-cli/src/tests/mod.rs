mod build;
mod helpers;
mod paths;
mod render;

use std::io::Error as IoError;

use alloc::collections::BTreeSet;
use proptest::collection::vec;
use proptest::prelude::any;
use proptest::prop_assert_eq;
use proptest::proptest;
use snafu::IntoError as _;
use uuid::Uuid;

use crate::command::paths::{
    entity_relations_path, entity_show_path, events_list_path, events_tail_path, memory_list_path,
    note_list_path,
};
use crate::command::render::{
    render_entity_context_compact, render_event_list_compact, render_graph_relations_compact,
    render_memory_list_compact, render_note_list_compact, render_task_dependency_compact,
    render_task_list_compact,
};
use crate::command::{
    build_mismatch_warning, dedup_task_ids, triage_has_changes, MemoryListPathArgs,
    TaskMetadataPatchArgs,
};
use crate::error::{ContractMismatchDetails, JsonContractMismatch};
use threadplane_core::{
    build_info, compare_build_info, EntityContext, EntityRecord, EpicRecord, EventKind,
    EventRecord, GraphRelation, MemoryAudience, MemoryImportance, MemoryKind, MemoryRecord,
    MemoryScope, NoteRecord, TaskClaimRecord, TaskDependencySummary, TaskListEntry, TaskMetadata,
    TaskPriority, TaskSummary,
};

fn sample_task_metadata() -> TaskMetadata {
    TaskMetadata {
        labels: vec!["workflow".to_owned(), "agent".to_owned()],
        owner: Some("codex".to_owned()),
        priority: TaskPriority::from_lossy("high"),
    }
}
