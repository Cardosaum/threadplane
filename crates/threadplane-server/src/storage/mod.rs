#![expect(
    clippy::arbitrary_source_item_ordering,
    reason = "Persistence helpers are grouped by capability and query workflow."
)]
#![expect(
    clippy::redundant_pub_crate,
    reason = "Persistence helpers are shared only inside this crate."
)]

mod dependencies;
mod events;
mod governance;
mod listings;
mod models;
mod reads;
mod transclusion;

use alloc::collections::BTreeMap;
use core::str::FromStr as _;

use sqlx::{query_as, FromRow, Postgres, QueryBuilder, Transaction};

use crate::prelude::*;
pub(crate) use dependencies::{
    append_task_dependency, fetch_dependency_chain, fetch_dependent_chain,
    fetch_direct_dependencies, fetch_direct_dependents, fetch_task_dependency_by_event_id,
    task_is_ready,
};
pub(crate) use events::{
    append_event, fetch_event_row_for_workspace, fetch_event_rows_after_cursor,
    fetch_event_rows_after_workspace_cursor, fetch_event_rows_for_workspace,
    fetch_projection_cursor, fetch_projection_status, record_projection_cursor, EventRow,
    ProjectionCursor,
};
#[cfg(test)]
pub(crate) use events::{build_projection_status, event_kind_name, parse_event_kind};
pub(crate) use governance::{
    ensure_workspace_governance, fetch_actor_public_keys, fetch_workspace_memberships,
    require_workspace_role, upsert_actor_public_key, upsert_workspace_membership,
    upsert_workspace_policy, workspace_supports_priority,
};
pub(crate) use listings::{
    build_task_list_entries, fetch_memories_for_listing, fetch_notes_for_listing,
    fetch_tasks_for_listing, MemoryListFilters, NoteListFilters, TaskListFilters,
};
pub(crate) use models::{
    ClaimRow, EpicRow, LinkRow, MemoryRow, NoteRow, TaskDependencyRow, TaskRow, TextEntityRow,
};
use models::{TaskDependencyListRow, TaskReadyRow, TransclusionGroupRow};
pub(crate) use reads::{
    fetch_active_claim, fetch_active_claim_tx, fetch_claim_by_event_id, fetch_entity_record,
    fetch_epic_by_event_id, fetch_epic_by_id, fetch_epic_by_id_tx, fetch_epic_for_task,
    fetch_epic_rows_for_workspace, fetch_link_by_event_id, fetch_memory_by_event_id,
    fetch_memory_by_id, fetch_memory_by_id_tx, fetch_note_by_event_id, fetch_note_by_id,
    fetch_note_by_id_tx, fetch_task_by_event_id, fetch_task_by_id, fetch_task_by_id_tx,
};
use threadplane_core::{
    epic_entity_ref, memory_entity_ref, normalize_memory_recall_triggers, normalize_memory_tags,
    normalize_task_labels, normalize_task_owner, note_entity_ref, parse_entity_ref,
    task_entity_ref, EntityRecord, EntityRef, EpicRecord, EventKind, EventRecord, MemoryAudience,
    MemoryImportance, MemoryKind, MemoryRecord, MemoryScope, NoteRecord, ProjectionStatus,
    TaskClaimRecord, TaskDependencySummary, TaskListEntry, TaskMetadata, TaskPriority, TaskRecord,
    TaskSummary, DEPENDS_ON_RELATION,
};
pub(crate) use transclusion::{
    prepare_xanadu_group, sync_transclusion_members, update_transclusion_group,
};

pub(crate) const NOTE_SELECT: &str = "
    SELECT
        note_id,
        event_id,
        workspace,
        author,
        title,
        body,
        transclusion_id,
        created_at,
        COALESCE(updated_at, created_at) AS updated_at
    FROM notes
";

pub(crate) const MEMORY_SELECT: &str = "
    SELECT
        memory_id,
        event_id,
        workspace,
        author,
        title,
        body,
        kind,
        scope,
        audience,
        importance,
        tags,
        recall_triggers,
        created_at,
        COALESCE(updated_at, created_at) AS updated_at
    FROM memories
";

pub(crate) const EPIC_SELECT: &str = "
    SELECT
        epic_id,
        event_id,
        workspace,
        author,
        title,
        body,
        created_at,
        COALESCE(updated_at, created_at) AS updated_at
    FROM epics
";

pub(crate) const TASK_SELECT: &str = "
    SELECT
        task_id,
        event_id,
        workspace,
        author,
        title,
        details,
        status,
        epic_id,
        priority,
        owner,
        labels,
        transclusion_id,
        created_at,
        COALESCE(updated_at, created_at) AS updated_at
    FROM tasks
";

pub(crate) const CLAIM_SELECT: &str = "
    SELECT
        claim_id,
        task_id,
        workspace,
        actor,
        event_id,
        claimed_at,
        expires_at,
        released_at
    FROM task_claims
";

pub(crate) const LINK_SELECT: &str = "
    SELECT
        link_id,
        event_id,
        workspace,
        actor,
        from_entity_ref,
        to_entity_ref,
        relation,
        is_xanadu,
        transclusion_id,
        created_at
    FROM links
";

pub(crate) fn unique_task_ids(task_ids: &[Uuid]) -> Vec<Uuid> {
    let mut unique_ids = Vec::new();
    for task_id in task_ids {
        if !unique_ids.contains(task_id) {
            unique_ids.push(*task_id);
        }
    }
    unique_ids
}
