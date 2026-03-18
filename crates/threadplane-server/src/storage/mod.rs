#![expect(
    clippy::arbitrary_source_item_ordering,
    reason = "Persistence helpers are grouped by capability and query workflow."
)]
#![expect(
    clippy::redundant_pub_crate,
    reason = "Persistence helpers are shared only inside this crate."
)]

mod events;
mod governance;
mod models;

use alloc::collections::BTreeMap;
use core::str::FromStr as _;

use sqlx::{query_as, FromRow, Postgres, QueryBuilder, Transaction};

use crate::prelude::*;
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

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct MemoryListFilters<'filter> {
    pub(crate) audience: Option<MemoryAudience>,
    pub(crate) importance: Option<MemoryImportance>,
    pub(crate) kind: Option<&'filter MemoryKind>,
    pub(crate) query: Option<&'filter str>,
    pub(crate) recall_trigger: Option<&'filter str>,
    pub(crate) tag: Option<&'filter str>,
}

pub(crate) async fn fetch_memories_for_listing(
    pool: &PgPool,
    workspace: &str,
    filters: MemoryListFilters<'_>,
    limit: i64,
) -> ServerResult<Vec<MemoryRow>> {
    let normalized_query = normalized_text_query(filters.query);
    let normalized_tag = normalized_memory_tag_filter(filters.tag);
    let normalized_recall_trigger = normalized_memory_recall_trigger_filter(filters.recall_trigger);
    let mut query = QueryBuilder::<Postgres>::new(MEMORY_SELECT);
    query.push(" WHERE workspace = ");
    query.push_bind(workspace);

    if let Some(kind) = filters.kind {
        query.push(" AND kind = ");
        query.push_bind(kind.as_str());
    }
    if let Some(audience) = filters.audience {
        query.push(" AND audience IN (");
        query.push_bind(audience.to_string());
        query.push(", ");
        query.push_bind(MemoryAudience::Both.to_string());
        query.push(")");
    }
    if let Some(importance) = filters.importance {
        query.push(" AND importance = ");
        query.push_bind(importance.to_string());
    }
    if let Some(tag) = normalized_tag {
        query.push(" AND tags @> ");
        query.push_bind(vec![tag]);
        query.push("::text[]");
    }
    if let Some(recall_trigger) = normalized_recall_trigger {
        query.push(" AND recall_triggers @> ");
        query.push_bind(vec![recall_trigger]);
        query.push("::text[]");
    }
    if let Some(search_query) = normalized_query {
        query.push(" AND (title ILIKE ");
        query.push_bind(format!("%{search_query}%"));
        query.push(" OR body ILIKE ");
        query.push_bind(format!("%{search_query}%"));
        query.push(")");
    }

    query.push(
        " ORDER BY CASE importance \
            WHEN 'critical' THEN 30 \
            WHEN 'high' THEN 20 \
            ELSE 10 \
          END DESC, updated_at DESC, created_at DESC",
    );
    query.push(" LIMIT ");
    query.push_bind(limit);

    query
        .build_query_as::<MemoryRow>()
        .fetch_all(pool)
        .await
        .map_err(Into::into)
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

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct NoteListFilters<'filter> {
    pub(crate) author: Option<&'filter str>,
    pub(crate) query: Option<&'filter str>,
}

pub(crate) async fn fetch_notes_for_listing(
    pool: &PgPool,
    workspace: &str,
    filters: NoteListFilters<'_>,
    limit: i64,
) -> ServerResult<Vec<NoteRow>> {
    let normalized_author = normalize_task_owner(filters.author.map(str::to_owned));
    let normalized_query = normalized_text_query(filters.query);
    let mut query = QueryBuilder::<Postgres>::new(NOTE_SELECT);
    query.push(" WHERE workspace = ");
    query.push_bind(workspace);

    if let Some(author) = normalized_author {
        query.push(" AND author = ");
        query.push_bind(author);
    }
    if let Some(search_query) = normalized_query {
        query.push(" AND (title ILIKE ");
        query.push_bind(format!("%{search_query}%"));
        query.push(" OR body ILIKE ");
        query.push_bind(format!("%{search_query}%"));
        query.push(")");
    }

    query.push(" ORDER BY updated_at DESC, created_at DESC");
    query.push(" LIMIT ");
    query.push_bind(limit);

    query
        .build_query_as::<NoteRow>()
        .fetch_all(pool)
        .await
        .map_err(Into::into)
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

fn normalized_text_query(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|candidate| !candidate.is_empty())
        .map(str::to_owned)
}

fn normalized_memory_tag_filter(value: Option<&str>) -> Option<String> {
    normalize_memory_tags(value.map(str::to_owned).into_iter().collect())
        .into_iter()
        .next()
}

fn normalized_memory_recall_trigger_filter(value: Option<&str>) -> Option<String> {
    normalize_memory_recall_triggers(value.map(str::to_owned).into_iter().collect())
        .into_iter()
        .next()
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

pub(crate) async fn fetch_task_dependency_by_event_id(
    pool: &PgPool,
    event_id: Uuid,
) -> ServerResult<Option<TaskDependencyRow>> {
    query_as(
        "
        SELECT task_id, depends_on_task_id
        FROM task_dependencies
        WHERE event_id = $1
        ",
    )
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

pub(crate) async fn append_task_dependency(
    tx: &mut Transaction<'_, Postgres>,
    workspace: &str,
    actor: &str,
    task_id: Uuid,
    depends_on_task_id: Uuid,
    created_at: DateTime<Utc>,
) -> ServerResult<Uuid> {
    if task_id == depends_on_task_id {
        return Err(ThreadplaneServerError::bad_request(
            "a task cannot depend on itself",
        ));
    }

    fetch_task_by_id_tx(tx, task_id, workspace).await?;
    fetch_task_by_id_tx(tx, depends_on_task_id, workspace).await?;

    let existing_edge: Option<(Uuid,)> = query_as(
        "
        SELECT task_id
        FROM task_dependencies
        WHERE task_id = $1
          AND depends_on_task_id = $2
        ",
    )
    .bind(task_id)
    .bind(depends_on_task_id)
    .fetch_optional(&mut **tx)
    .await?;
    if existing_edge.is_some() {
        return Err(ThreadplaneServerError::conflict(
            "dependency edge already exists",
        ));
    }

    if dependency_would_create_cycle(tx, task_id, depends_on_task_id).await? {
        return Err(ThreadplaneServerError::conflict(
            "dependency would create a cycle in the task DAG",
        ));
    }

    let payload = json!({
        "workspace": workspace,
        "actor": actor,
        "task_id": task_id,
        "depends_on_task_id": depends_on_task_id,
        "relation": DEPENDS_ON_RELATION,
    });
    let event_id = append_event(
        tx,
        workspace,
        actor,
        EventKind::TaskDependencyDeclared,
        &payload,
        created_at,
    )
    .await?;

    sqlx::query(
        "
        INSERT INTO task_dependencies (
            task_id,
            depends_on_task_id,
            workspace,
            actor,
            event_id,
            created_at
        )
        VALUES ($1, $2, $3, $4, $5, $6)
        ",
    )
    .bind(task_id)
    .bind(depends_on_task_id)
    .bind(workspace)
    .bind(actor)
    .bind(event_id)
    .bind(created_at)
    .execute(&mut **tx)
    .await?;

    sqlx::query(
        "
        UPDATE tasks
        SET updated_at = $2
        WHERE task_id = $1
        ",
    )
    .bind(task_id)
    .bind(created_at)
    .execute(&mut **tx)
    .await?;

    Ok(event_id)
}

pub(crate) async fn dependency_would_create_cycle(
    tx: &mut Transaction<'_, Postgres>,
    task_id: Uuid,
    depends_on_task_id: Uuid,
) -> ServerResult<bool> {
    let cycle_row: Option<(Uuid,)> = query_as(
        "
        WITH RECURSIVE reachable(task_id) AS (
            SELECT depends_on_task_id
            FROM task_dependencies
            WHERE task_id = $1
            UNION
            SELECT td.depends_on_task_id
            FROM task_dependencies td
            JOIN reachable r ON td.task_id = r.task_id
        )
        SELECT task_id
        FROM reachable
        WHERE task_id = $2
        LIMIT 1
        ",
    )
    .bind(depends_on_task_id)
    .bind(task_id)
    .fetch_optional(&mut **tx)
    .await?;

    Ok(cycle_row.is_some())
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

#[derive(Debug, Clone, Default)]
pub(crate) struct TaskListFilters<'filter> {
    pub(crate) epic_id: Option<Uuid>,
    pub(crate) label: Option<&'filter str>,
    pub(crate) owner: Option<&'filter str>,
    pub(crate) priority: Option<TaskPriority>,
    pub(crate) ready_only: bool,
    pub(crate) status: Option<&'filter str>,
}

pub(crate) async fn fetch_tasks_for_listing(
    pool: &PgPool,
    workspace: &str,
    filters: TaskListFilters<'_>,
    limit: Option<i64>,
) -> ServerResult<Vec<TaskRow>> {
    if let Some(filter_value) = filters.status {
        if !matches!(filter_value, "open" | "claimed" | "completed") {
            return Err(ThreadplaneServerError::bad_request(format!(
                "unsupported task status filter {filter_value}"
            )));
        }
    }
    if filters.ready_only && matches!(filters.status, Some("claimed" | "completed")) {
        return Ok(Vec::new());
    }

    let normalized_owner = normalize_task_owner(filters.owner.map(str::to_owned));
    let normalized_label =
        normalize_task_labels(filters.label.map(str::to_owned).into_iter().collect())
            .into_iter()
            .next();
    let mut query = QueryBuilder::<Postgres>::new(TASK_SELECT);
    query.push(" WHERE workspace = ");
    query.push_bind(workspace);

    if let Some(filter_value) = filters.status {
        query.push(" AND status = ");
        query.push_bind(filter_value);
    }
    if let Some(selected_epic_id) = filters.epic_id {
        query.push(" AND epic_id = ");
        query.push_bind(selected_epic_id);
    }
    if let Some(selected_priority) = filters.priority {
        query.push(" AND priority = ");
        query.push_bind(selected_priority.to_string());
    }
    if let Some(selected_owner) = normalized_owner {
        query.push(" AND owner = ");
        query.push_bind(selected_owner);
    }
    if let Some(selected_label) = normalized_label {
        query.push(" AND labels @> ARRAY[");
        query.push_bind(selected_label);
        query.push("]::text[]");
    }
    if filters.ready_only {
        if filters.status.is_none() {
            query.push(" AND status = 'open'");
        }
        query.push(
            "
            AND NOT EXISTS (
                SELECT 1
                FROM task_dependencies td
                JOIN tasks dependency ON dependency.task_id = td.depends_on_task_id
                WHERE td.task_id = tasks.task_id
                  AND dependency.status <> 'completed'
            )
            ",
        );
        query.push(
            "
            ORDER BY
                COALESCE(
                    (
                        SELECT wp.rank
                        FROM workspace_priorities wp
                        WHERE wp.workspace = tasks.workspace
                          AND wp.name = tasks.priority
                    ),
                    0
                ) DESC,
                updated_at DESC,
                created_at DESC
            ",
        );
    } else {
        query.push(" ORDER BY created_at DESC");
    }
    if let Some(query_limit) = limit {
        query.push(" LIMIT ");
        query.push_bind(query_limit);
    }

    let rows = query.build_query_as::<TaskRow>().fetch_all(pool).await?;
    Ok(rows)
}

pub(crate) async fn build_task_list_entries(
    pool: &PgPool,
    tasks: Vec<TaskRow>,
) -> ServerResult<Vec<TaskListEntry>> {
    let task_ids: Vec<Uuid> = tasks.iter().map(|task| task.task_id).collect();
    let mut active_claims = fetch_active_claims_for_tasks(pool, &task_ids).await?;
    let mut dependencies = fetch_direct_dependencies_for_tasks(pool, &task_ids).await?;
    let mut dependents = fetch_direct_dependents_for_tasks(pool, &task_ids).await?;
    let mut epics = fetch_epics_for_tasks(pool, &tasks).await?;
    let ready_states = fetch_ready_states_for_tasks(pool, &task_ids).await?;
    let mut entries = Vec::with_capacity(tasks.len());
    for task in tasks {
        let task_id = task.task_id;
        let epic_id = task.epic_id;
        entries.push(TaskListEntry {
            active_claim: active_claims.remove(&task_id),
            dependencies: dependencies.remove(&task_id).unwrap_or_default(),
            dependents: dependents.remove(&task_id).unwrap_or_default(),
            epic: epic_id.and_then(|value| epics.remove(&value)),
            ready: ready_states.get(&task_id).copied().unwrap_or(false),
            task: task.into(),
        });
    }
    Ok(entries)
}

async fn fetch_active_claims_for_tasks(
    pool: &PgPool,
    task_ids: &[Uuid],
) -> ServerResult<BTreeMap<Uuid, TaskClaimRecord>> {
    if task_ids.is_empty() {
        return Ok(BTreeMap::new());
    }

    let rows: Vec<ClaimRow> = query_as(
        "
        SELECT DISTINCT ON (task_id)
            claim_id,
            task_id,
            workspace,
            actor,
            event_id,
            claimed_at,
            expires_at,
            released_at
        FROM task_claims
        WHERE task_id = ANY($1)
          AND released_at IS NULL
          AND expires_at > now()
        ORDER BY task_id, claimed_at DESC
        ",
    )
    .bind(task_ids)
    .fetch_all(pool)
    .await?;

    let mut claims = BTreeMap::new();
    for row in rows {
        claims.insert(row.task_id, row.into());
    }
    Ok(claims)
}

async fn fetch_dependency_rows_for_tasks(
    pool: &PgPool,
    task_ids: &[Uuid],
    reverse: bool,
) -> ServerResult<BTreeMap<Uuid, Vec<TaskDependencySummary>>> {
    if task_ids.is_empty() {
        return Ok(BTreeMap::new());
    }

    let sql = if reverse {
        "
        SELECT
            td.depends_on_task_id AS source_task_id,
            t.task_id AS dependency_id,
            t.title,
            t.status
        FROM task_dependencies td
        JOIN tasks t ON t.task_id = td.task_id
        WHERE td.depends_on_task_id = ANY($1)
        ORDER BY td.depends_on_task_id, t.created_at DESC
        "
    } else {
        "
        SELECT
            td.task_id AS source_task_id,
            t.task_id AS dependency_id,
            t.title,
            t.status
        FROM task_dependencies td
        JOIN tasks t ON t.task_id = td.depends_on_task_id
        WHERE td.task_id = ANY($1)
        ORDER BY td.task_id, t.created_at DESC
        "
    };

    let rows: Vec<TaskDependencyListRow> = query_as(sql).bind(task_ids).fetch_all(pool).await?;
    let mut dependencies = BTreeMap::new();
    for row in rows {
        dependencies
            .entry(row.source_task_id)
            .or_insert_with(Vec::new)
            .push(TaskDependencySummary {
                depth: 1,
                entity_ref: task_entity_ref(row.dependency_id),
                status: row.status,
                task_id: row.dependency_id,
                title: row.title,
            });
    }

    Ok(dependencies)
}

async fn fetch_direct_dependencies_for_tasks(
    pool: &PgPool,
    task_ids: &[Uuid],
) -> ServerResult<BTreeMap<Uuid, Vec<TaskDependencySummary>>> {
    fetch_dependency_rows_for_tasks(pool, task_ids, false).await
}

async fn fetch_direct_dependents_for_tasks(
    pool: &PgPool,
    task_ids: &[Uuid],
) -> ServerResult<BTreeMap<Uuid, Vec<TaskDependencySummary>>> {
    fetch_dependency_rows_for_tasks(pool, task_ids, true).await
}

async fn fetch_epics_for_tasks(
    pool: &PgPool,
    tasks: &[TaskRow],
) -> ServerResult<BTreeMap<Uuid, EpicRecord>> {
    let mut epic_ids = Vec::new();
    for task in tasks {
        if let Some(epic_id) = task.epic_id {
            if !epic_ids.contains(&epic_id) {
                epic_ids.push(epic_id);
            }
        }
    }
    if epic_ids.is_empty() {
        return Ok(BTreeMap::new());
    }

    let rows: Vec<EpicRow> = query_as(&format!("{EPIC_SELECT} WHERE epic_id = ANY($1)"))
        .bind(epic_ids)
        .fetch_all(pool)
        .await?;

    let mut epics = BTreeMap::new();
    for row in rows {
        epics.insert(row.epic_id, row.into());
    }
    Ok(epics)
}

async fn fetch_ready_states_for_tasks(
    pool: &PgPool,
    task_ids: &[Uuid],
) -> ServerResult<BTreeMap<Uuid, bool>> {
    if task_ids.is_empty() {
        return Ok(BTreeMap::new());
    }

    let rows: Vec<TaskReadyRow> = query_as(
        "
        SELECT
            t.task_id,
            CASE
                WHEN t.status <> 'open' THEN false
                WHEN EXISTS (
                    SELECT 1
                    FROM task_dependencies td
                    JOIN tasks dependency ON dependency.task_id = td.depends_on_task_id
                    WHERE td.task_id = t.task_id
                      AND dependency.status <> 'completed'
                ) THEN false
                ELSE true
            END AS ready
        FROM tasks t
        WHERE t.task_id = ANY($1)
        ",
    )
    .bind(task_ids)
    .fetch_all(pool)
    .await?;

    let mut ready_states = BTreeMap::new();
    for row in rows {
        ready_states.insert(row.task_id, row.ready);
    }
    Ok(ready_states)
}

pub(crate) async fn fetch_direct_dependencies(
    pool: &PgPool,
    task_id: Uuid,
) -> ServerResult<Vec<TaskDependencySummary>> {
    fetch_dependency_rows(pool, task_id, false).await
}

pub(crate) async fn fetch_direct_dependents(
    pool: &PgPool,
    task_id: Uuid,
) -> ServerResult<Vec<TaskDependencySummary>> {
    fetch_dependency_rows(pool, task_id, true).await
}

pub(crate) async fn fetch_dependency_chain(
    pool: &PgPool,
    task_id: Uuid,
) -> ServerResult<Vec<TaskDependencySummary>> {
    fetch_dependency_chain_rows(pool, task_id, false).await
}

pub(crate) async fn fetch_dependent_chain(
    pool: &PgPool,
    task_id: Uuid,
) -> ServerResult<Vec<TaskDependencySummary>> {
    fetch_dependency_chain_rows(pool, task_id, true).await
}

async fn fetch_dependency_rows(
    pool: &PgPool,
    task_id: Uuid,
    reverse: bool,
) -> ServerResult<Vec<TaskDependencySummary>> {
    let sql = if reverse {
        "
        SELECT t.task_id, t.title, t.status
        FROM task_dependencies td
        JOIN tasks t ON t.task_id = td.task_id
        WHERE td.depends_on_task_id = $1
        ORDER BY t.created_at DESC
        "
    } else {
        "
        SELECT t.task_id, t.title, t.status
        FROM task_dependencies td
        JOIN tasks t ON t.task_id = td.depends_on_task_id
        WHERE td.task_id = $1
        ORDER BY t.created_at DESC
        "
    };

    let rows: Vec<(Uuid, String, String)> = query_as(sql).bind(task_id).fetch_all(pool).await?;

    Ok(rows
        .into_iter()
        .map(|(dependency_id, title, status)| TaskDependencySummary {
            depth: 1,
            entity_ref: task_entity_ref(dependency_id),
            status,
            task_id: dependency_id,
            title,
        })
        .collect())
}

async fn fetch_dependency_chain_rows(
    pool: &PgPool,
    task_id: Uuid,
    reverse: bool,
) -> ServerResult<Vec<TaskDependencySummary>> {
    let sql = if reverse {
        "
        WITH RECURSIVE dependency_chain(task_id, depth) AS (
            SELECT td.task_id, 1
            FROM task_dependencies td
            WHERE td.depends_on_task_id = $1
            UNION
            SELECT td.task_id, dependency_chain.depth + 1
            FROM task_dependencies td
            JOIN dependency_chain ON td.depends_on_task_id = dependency_chain.task_id
        )
        SELECT t.task_id, t.title, t.status, MIN(dependency_chain.depth) AS depth
        FROM dependency_chain
        JOIN tasks t ON t.task_id = dependency_chain.task_id
        GROUP BY t.task_id, t.title, t.status
        ORDER BY depth, t.created_at DESC
        "
    } else {
        "
        WITH RECURSIVE dependency_chain(task_id, depth) AS (
            SELECT td.depends_on_task_id, 1
            FROM task_dependencies td
            WHERE td.task_id = $1
            UNION
            SELECT td.depends_on_task_id, dependency_chain.depth + 1
            FROM task_dependencies td
            JOIN dependency_chain ON td.task_id = dependency_chain.task_id
        )
        SELECT t.task_id, t.title, t.status, MIN(dependency_chain.depth) AS depth
        FROM dependency_chain
        JOIN tasks t ON t.task_id = dependency_chain.task_id
        GROUP BY t.task_id, t.title, t.status
        ORDER BY depth, t.created_at DESC
        "
    };

    let rows: Vec<(Uuid, String, String, i32)> =
        query_as(sql).bind(task_id).fetch_all(pool).await?;

    Ok(rows
        .into_iter()
        .map(
            |(dependency_id, title, status, depth)| TaskDependencySummary {
                depth,
                entity_ref: task_entity_ref(dependency_id),
                status,
                task_id: dependency_id,
                title,
            },
        )
        .collect())
}

pub(crate) async fn task_is_ready(pool: &PgPool, task_id: Uuid) -> ServerResult<bool> {
    let task = fetch_task_by_id(pool, task_id).await?;
    if task.status != "open" {
        return Ok(false);
    }

    let unresolved: (i64,) = query_as(
        "
        SELECT COUNT(*)
        FROM task_dependencies td
        JOIN tasks dependency ON dependency.task_id = td.depends_on_task_id
        WHERE td.task_id = $1
          AND dependency.status <> 'completed'
        ",
    )
    .bind(task_id)
    .fetch_one(pool)
    .await?;

    Ok(unresolved.0 == 0)
}

pub(crate) async fn fetch_text_entity_by_ref_tx(
    tx: &mut Transaction<'_, Postgres>,
    workspace: &str,
    entity_ref: &str,
) -> ServerResult<TextEntityRow> {
    match parse_entity_ref(entity_ref) {
        Some(EntityRef::Epic(_) | EntityRef::Memory(_)) => {
            Err(ThreadplaneServerError::bad_request(format!(
                "non-textual entity refs cannot join xanadu groups: {entity_ref}"
            )))
        }
        Some(EntityRef::Note(note_id)) => Ok(TextEntityRow::Note(
            fetch_note_by_id_tx(tx, note_id, workspace).await?,
        )),
        Some(EntityRef::Task(task_id)) => Ok(TextEntityRow::Task(
            fetch_task_by_id_tx(tx, task_id, workspace).await?,
        )),
        None => Err(ThreadplaneServerError::bad_request(format!(
            "unsupported entity ref {entity_ref}"
        ))),
    }
}

pub(crate) async fn group_exists(
    tx: &mut Transaction<'_, Postgres>,
    transclusion_id: Uuid,
) -> ServerResult<bool> {
    let exists: Option<(Uuid,)> =
        query_as("SELECT transclusion_id FROM transclusion_groups WHERE transclusion_id = $1")
            .bind(transclusion_id)
            .fetch_optional(&mut **tx)
            .await?;
    Ok(exists.is_some())
}

pub(crate) async fn insert_transclusion_group(
    tx: &mut Transaction<'_, Postgres>,
    transclusion_id: Uuid,
    workspace: &str,
    actor: &str,
    title: &str,
    content: &str,
    now: DateTime<Utc>,
) -> ServerResult<()> {
    sqlx::query(
        "
        INSERT INTO transclusion_groups (
            transclusion_id,
            workspace,
            created_by,
            title,
            content,
            created_at,
            updated_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $6)
        ",
    )
    .bind(transclusion_id)
    .bind(workspace)
    .bind(actor)
    .bind(title)
    .bind(content)
    .bind(now)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub(crate) async fn update_transclusion_group(
    tx: &mut Transaction<'_, Postgres>,
    transclusion_id: Uuid,
    workspace: &str,
    _actor: &str,
    title: &str,
    content: &str,
    now: DateTime<Utc>,
) -> ServerResult<()> {
    sqlx::query(
        "
        UPDATE transclusion_groups
        SET workspace = $2,
            title = $3,
            content = $4,
            updated_at = $5
        WHERE transclusion_id = $1
        ",
    )
    .bind(transclusion_id)
    .bind(workspace)
    .bind(title)
    .bind(content)
    .bind(now)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub(crate) async fn move_group_members(
    tx: &mut Transaction<'_, Postgres>,
    from_group_id: Uuid,
    to_group_id: Uuid,
) -> ServerResult<()> {
    sqlx::query("UPDATE notes SET transclusion_id = $2 WHERE transclusion_id = $1")
        .bind(from_group_id)
        .bind(to_group_id)
        .execute(&mut **tx)
        .await?;
    sqlx::query("UPDATE tasks SET transclusion_id = $2 WHERE transclusion_id = $1")
        .bind(from_group_id)
        .bind(to_group_id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

pub(crate) async fn set_entity_transclusion(
    tx: &mut Transaction<'_, Postgres>,
    entity: &TextEntityRow,
    transclusion_id: Uuid,
) -> ServerResult<()> {
    match entity {
        TextEntityRow::Note(note) => {
            sqlx::query("UPDATE notes SET transclusion_id = $2 WHERE note_id = $1")
                .bind(note.note_id)
                .bind(transclusion_id)
                .execute(&mut **tx)
                .await?;
        }
        TextEntityRow::Task(task) => {
            sqlx::query("UPDATE tasks SET transclusion_id = $2 WHERE task_id = $1")
                .bind(task.task_id)
                .bind(transclusion_id)
                .execute(&mut **tx)
                .await?;
        }
    }
    Ok(())
}

pub(crate) async fn sync_transclusion_members(
    tx: &mut Transaction<'_, Postgres>,
    transclusion_id: Uuid,
) -> ServerResult<()> {
    let group: TransclusionGroupRow = query_as(
        "
        SELECT transclusion_id, workspace, title, content, created_at, updated_at
        FROM transclusion_groups
        WHERE transclusion_id = $1
        ",
    )
    .bind(transclusion_id)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| ThreadplaneServerError::not_found("transclusion group not found"))?;

    sqlx::query(
        "
        UPDATE notes
        SET title = $2,
            body = $3,
            updated_at = $4
        WHERE transclusion_id = $1
        ",
    )
    .bind(transclusion_id)
    .bind(&group.title)
    .bind(&group.content)
    .bind(group.updated_at)
    .execute(&mut **tx)
    .await?;

    sqlx::query(
        "
        UPDATE tasks
        SET title = $2,
            details = $3,
            updated_at = $4
        WHERE transclusion_id = $1
        ",
    )
    .bind(transclusion_id)
    .bind(&group.title)
    .bind(&group.content)
    .bind(group.updated_at)
    .execute(&mut **tx)
    .await?;

    Ok(())
}

pub(crate) struct XanaduGroup {
    pub(crate) canonical_group_id: Uuid,
    pub(crate) merged_group_id: Option<Uuid>,
}

pub(crate) async fn prepare_xanadu_group(
    tx: &mut Transaction<'_, Postgres>,
    request: &threadplane_core::CreateXanaduLinkRequest,
    created_at: DateTime<Utc>,
) -> ServerResult<XanaduGroup> {
    let from = fetch_text_entity_by_ref_tx(tx, &request.workspace, &request.from).await?;
    let to = fetch_text_entity_by_ref_tx(tx, &request.workspace, &request.to).await?;
    let canonical_group_id = from
        .transclusion_id()
        .or_else(|| to.transclusion_id())
        .unwrap_or_else(Uuid::new_v4);
    let merged_group_id = match (from.transclusion_id(), to.transclusion_id()) {
        (Some(left), Some(right)) if left != right => Some(right),
        _ => None,
    };

    upsert_xanadu_group(
        tx,
        request,
        &from,
        canonical_group_id,
        merged_group_id,
        created_at,
    )
    .await?;
    set_entity_transclusion(tx, &from, canonical_group_id).await?;
    set_entity_transclusion(tx, &to, canonical_group_id).await?;
    sync_transclusion_members(tx, canonical_group_id).await?;

    Ok(XanaduGroup {
        canonical_group_id,
        merged_group_id,
    })
}

async fn upsert_xanadu_group(
    tx: &mut Transaction<'_, Postgres>,
    request: &threadplane_core::CreateXanaduLinkRequest,
    from: &TextEntityRow,
    canonical_group_id: Uuid,
    merged_group_id: Option<Uuid>,
    created_at: DateTime<Utc>,
) -> ServerResult<()> {
    let source_title = from.title().to_owned();
    let source_content = from.content().to_owned();

    if group_exists(tx, canonical_group_id).await? {
        update_transclusion_group(
            tx,
            canonical_group_id,
            &request.workspace,
            &request.actor,
            &source_title,
            &source_content,
            created_at,
        )
        .await?;
    } else {
        insert_transclusion_group(
            tx,
            canonical_group_id,
            &request.workspace,
            &request.actor,
            &source_title,
            &source_content,
            created_at,
        )
        .await?;
    }

    if let Some(group_id) = merged_group_id {
        move_group_members(tx, group_id, canonical_group_id).await?;
        sqlx::query("DELETE FROM transclusion_groups WHERE transclusion_id = $1")
            .bind(group_id)
            .execute(&mut **tx)
            .await?;
    }

    Ok(())
}
