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

pub(crate) async fn fetch_epic_rows_for_workspace(
    pool: &PgPool,
    workspace: &str,
) -> ServerResult<Vec<EpicRow>> {
    query_as(&format!(
        "
        {EPIC_SELECT}
        WHERE workspace = $1
        ORDER BY created_at DESC
        "
    ))
    .bind(workspace)
    .fetch_all(pool)
    .await
    .map_err(Into::into)
}

pub(crate) async fn fetch_epic_by_id(pool: &PgPool, epic_id: Uuid) -> ServerResult<EpicRow> {
    query_as(&format!("{EPIC_SELECT} WHERE epic_id = $1"))
        .bind(epic_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| ThreadplaneServerError::not_found("epic not found"))
}

pub(crate) async fn fetch_epic_by_event_id(
    pool: &PgPool,
    event_id: Uuid,
) -> ServerResult<Option<EpicRow>> {
    query_as(&format!("{EPIC_SELECT} WHERE event_id = $1"))
        .bind(event_id)
        .fetch_optional(pool)
        .await
        .map_err(Into::into)
}

pub(crate) async fn fetch_epic_by_id_tx(
    tx: &mut Transaction<'_, Postgres>,
    epic_id: Uuid,
    workspace: &str,
) -> ServerResult<EpicRow> {
    query_as(&format!(
        "{EPIC_SELECT} WHERE epic_id = $1 AND workspace = $2"
    ))
    .bind(epic_id)
    .bind(workspace)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| ThreadplaneServerError::not_found("epic not found"))
}

pub(crate) async fn fetch_note_by_id(pool: &PgPool, note_id: Uuid) -> ServerResult<NoteRow> {
    query_as(&format!("{NOTE_SELECT} WHERE note_id = $1"))
        .bind(note_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| ThreadplaneServerError::not_found("note not found"))
}

pub(crate) async fn fetch_memory_by_id(pool: &PgPool, memory_id: Uuid) -> ServerResult<MemoryRow> {
    query_as(&format!("{MEMORY_SELECT} WHERE memory_id = $1"))
        .bind(memory_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| ThreadplaneServerError::not_found("memory not found"))
}

pub(crate) async fn fetch_memory_by_event_id(
    pool: &PgPool,
    event_id: Uuid,
) -> ServerResult<Option<MemoryRow>> {
    query_as(&format!("{MEMORY_SELECT} WHERE event_id = $1"))
        .bind(event_id)
        .fetch_optional(pool)
        .await
        .map_err(Into::into)
}

pub(crate) async fn fetch_memory_by_id_tx(
    tx: &mut Transaction<'_, Postgres>,
    memory_id: Uuid,
    workspace: &str,
) -> ServerResult<MemoryRow> {
    query_as(&format!(
        "{MEMORY_SELECT} WHERE memory_id = $1 AND workspace = $2"
    ))
    .bind(memory_id)
    .bind(workspace)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| ThreadplaneServerError::not_found("memory not found"))
}

pub(crate) async fn fetch_note_by_event_id(
    pool: &PgPool,
    event_id: Uuid,
) -> ServerResult<Option<NoteRow>> {
    query_as(&format!("{NOTE_SELECT} WHERE event_id = $1"))
        .bind(event_id)
        .fetch_optional(pool)
        .await
        .map_err(Into::into)
}

pub(crate) async fn fetch_note_by_id_tx(
    tx: &mut Transaction<'_, Postgres>,
    note_id: Uuid,
    workspace: &str,
) -> ServerResult<NoteRow> {
    query_as(&format!(
        "{NOTE_SELECT} WHERE note_id = $1 AND workspace = $2"
    ))
    .bind(note_id)
    .bind(workspace)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| ThreadplaneServerError::not_found("note not found"))
}

pub(crate) async fn fetch_task_by_id(pool: &PgPool, task_id: Uuid) -> ServerResult<TaskRow> {
    query_as(&format!("{TASK_SELECT} WHERE task_id = $1"))
        .bind(task_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| ThreadplaneServerError::not_found("task not found"))
}

pub(crate) async fn fetch_entity_record(
    pool: &PgPool,
    entity_ref: &str,
) -> ServerResult<EntityRecord> {
    match parse_entity_ref(entity_ref) {
        Some(EntityRef::Epic(epic_id)) => {
            let epic = fetch_epic_by_id(pool, epic_id).await?;
            Ok(EntityRecord::Epic(EpicRecord::from(epic)))
        }
        Some(EntityRef::Memory(memory_id)) => {
            let memory = fetch_memory_by_id(pool, memory_id).await?;
            Ok(EntityRecord::Memory(MemoryRecord::try_from(memory)?))
        }
        Some(EntityRef::Note(note_id)) => {
            let note = fetch_note_by_id(pool, note_id).await?;
            Ok(EntityRecord::Note(NoteRecord::from(note)))
        }
        Some(EntityRef::Task(task_id)) => {
            let task = fetch_task_by_id(pool, task_id).await?;
            Ok(EntityRecord::Task(TaskRecord::from(task)))
        }
        None => Err(ThreadplaneServerError::bad_request(format!(
            "invalid entity ref: {entity_ref}"
        ))),
    }
}

pub(crate) async fn fetch_task_by_event_id(
    pool: &PgPool,
    event_id: Uuid,
) -> ServerResult<Option<TaskRow>> {
    query_as(&format!("{TASK_SELECT} WHERE event_id = $1"))
        .bind(event_id)
        .fetch_optional(pool)
        .await
        .map_err(Into::into)
}

pub(crate) async fn fetch_task_by_id_tx(
    tx: &mut Transaction<'_, Postgres>,
    task_id: Uuid,
    workspace: &str,
) -> ServerResult<TaskRow> {
    query_as(&format!(
        "{TASK_SELECT} WHERE task_id = $1 AND workspace = $2"
    ))
    .bind(task_id)
    .bind(workspace)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| ThreadplaneServerError::not_found("task not found"))
}

pub(crate) async fn fetch_active_claim(
    pool: &PgPool,
    task_id: Uuid,
) -> ServerResult<Option<ClaimRow>> {
    query_as(
        "
        SELECT claim_id, task_id, workspace, actor, event_id, claimed_at, expires_at, released_at
        FROM task_claims
        WHERE task_id = $1
          AND released_at IS NULL
          AND expires_at > now()
        ORDER BY claimed_at DESC
        LIMIT 1
        ",
    )
    .bind(task_id)
    .fetch_optional(pool)
    .await
    .map_err(Into::into)
}

pub(crate) async fn fetch_claim_by_event_id(
    pool: &PgPool,
    event_id: Uuid,
) -> ServerResult<Option<ClaimRow>> {
    query_as(&format!("{CLAIM_SELECT} WHERE event_id = $1"))
        .bind(event_id)
        .fetch_optional(pool)
        .await
        .map_err(Into::into)
}

pub(crate) async fn fetch_link_by_event_id(
    pool: &PgPool,
    event_id: Uuid,
) -> ServerResult<Option<LinkRow>> {
    query_as(&format!("{LINK_SELECT} WHERE event_id = $1"))
        .bind(event_id)
        .fetch_optional(pool)
        .await
        .map_err(Into::into)
}

pub(crate) async fn fetch_active_claim_tx(
    tx: &mut Transaction<'_, Postgres>,
    task_id: Uuid,
) -> ServerResult<Option<ClaimRow>> {
    query_as(
        "
        SELECT claim_id, task_id, workspace, actor, event_id, claimed_at, expires_at, released_at
        FROM task_claims
        WHERE task_id = $1
          AND released_at IS NULL
          AND expires_at > now()
        ORDER BY claimed_at DESC
        LIMIT 1
        ",
    )
    .bind(task_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(Into::into)
}

pub(crate) async fn fetch_epic_for_task(
    pool: &PgPool,
    task: &TaskRow,
) -> ServerResult<Option<EpicRecord>> {
    if let Some(epic_id) = task.epic_id {
        return fetch_epic_by_id(pool, epic_id)
            .await
            .map(EpicRecord::from)
            .map(Some);
    }

    Ok(None)
}
