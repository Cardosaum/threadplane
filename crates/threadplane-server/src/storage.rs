#![expect(
    clippy::arbitrary_source_item_ordering,
    reason = "Persistence helpers are grouped by capability and query workflow."
)]
#![expect(
    clippy::redundant_pub_crate,
    reason = "Persistence helpers are shared only inside this crate."
)]

use core::str::FromStr as _;

use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use sqlx::{query_as, FromRow, PgPool, Postgres, QueryBuilder, Transaction};
use uuid::Uuid;

use crate::error::{ServerResult, ThreadplaneServerError};
use threadplane_core::{
    epic_entity_ref, normalize_task_labels, normalize_task_owner, note_entity_ref,
    parse_entity_ref, task_entity_ref, EntityRef, EpicRecord, EventKind, EventRecord,
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

pub(crate) async fn ensure_schema(pool: &PgPool) -> ServerResult<()> {
    for statement in schema_statements() {
        sqlx::query(statement).execute(pool).await?;
    }

    Ok(())
}

#[expect(
    clippy::too_many_lines,
    reason = "Schema bootstrap is intentionally centralized as one ordered statement list."
)]
pub(crate) const fn schema_statements() -> &'static [&'static str] {
    &[
        "
        CREATE TABLE IF NOT EXISTS events (
            event_id UUID PRIMARY KEY,
            workspace TEXT NOT NULL,
            actor TEXT NOT NULL,
            kind TEXT NOT NULL,
            payload JSONB NOT NULL,
            created_at TIMESTAMPTZ NOT NULL
        )
        ",
        "
        CREATE TABLE IF NOT EXISTS epics (
            epic_id UUID PRIMARY KEY,
            event_id UUID NOT NULL REFERENCES events(event_id),
            workspace TEXT NOT NULL,
            author TEXT NOT NULL,
            title TEXT NOT NULL,
            body TEXT NOT NULL,
            created_at TIMESTAMPTZ NOT NULL,
            updated_at TIMESTAMPTZ NOT NULL
        )
        ",
        "
        CREATE TABLE IF NOT EXISTS notes (
            note_id UUID PRIMARY KEY,
            event_id UUID NOT NULL REFERENCES events(event_id),
            workspace TEXT NOT NULL,
            author TEXT NOT NULL,
            title TEXT NOT NULL,
            body TEXT NOT NULL,
            created_at TIMESTAMPTZ NOT NULL
        )
        ",
        "
        CREATE TABLE IF NOT EXISTS tasks (
            task_id UUID PRIMARY KEY,
            event_id UUID NOT NULL REFERENCES events(event_id),
            workspace TEXT NOT NULL,
            author TEXT NOT NULL,
            title TEXT NOT NULL,
            details TEXT NOT NULL,
            status TEXT NOT NULL,
            epic_id UUID NULL,
            priority TEXT NOT NULL DEFAULT 'medium',
            owner TEXT NULL,
            labels TEXT[] NOT NULL DEFAULT '{}',
            created_at TIMESTAMPTZ NOT NULL,
            updated_at TIMESTAMPTZ NOT NULL
        )
        ",
        "
        CREATE TABLE IF NOT EXISTS task_claims (
            claim_id UUID PRIMARY KEY,
            task_id UUID NOT NULL REFERENCES tasks(task_id),
            workspace TEXT NOT NULL,
            actor TEXT NOT NULL,
            event_id UUID NOT NULL REFERENCES events(event_id),
            claimed_at TIMESTAMPTZ NOT NULL,
            expires_at TIMESTAMPTZ NOT NULL,
            released_at TIMESTAMPTZ NULL
        )
        ",
        "
        CREATE TABLE IF NOT EXISTS links (
            link_id UUID PRIMARY KEY,
            event_id UUID NOT NULL REFERENCES events(event_id),
            workspace TEXT NOT NULL,
            actor TEXT NOT NULL,
            from_entity_ref TEXT NOT NULL,
            to_entity_ref TEXT NOT NULL,
            relation TEXT NOT NULL,
            created_at TIMESTAMPTZ NOT NULL
        )
        ",
        "
        CREATE TABLE IF NOT EXISTS task_dependencies (
            task_id UUID NOT NULL REFERENCES tasks(task_id),
            depends_on_task_id UUID NOT NULL REFERENCES tasks(task_id),
            workspace TEXT NOT NULL,
            actor TEXT NOT NULL,
            event_id UUID NOT NULL REFERENCES events(event_id),
            created_at TIMESTAMPTZ NOT NULL,
            PRIMARY KEY (task_id, depends_on_task_id)
        )
        ",
        "
        CREATE TABLE IF NOT EXISTS transclusion_groups (
            transclusion_id UUID PRIMARY KEY,
            workspace TEXT NOT NULL,
            created_by TEXT NOT NULL,
            title TEXT NOT NULL,
            content TEXT NOT NULL,
            created_at TIMESTAMPTZ NOT NULL,
            updated_at TIMESTAMPTZ NOT NULL
        )
        ",
        "ALTER TABLE notes ADD COLUMN IF NOT EXISTS transclusion_id UUID",
        "ALTER TABLE notes ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ",
        "ALTER TABLE epics ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ",
        "UPDATE epics SET updated_at = created_at WHERE updated_at IS NULL",
        "ALTER TABLE tasks ADD COLUMN IF NOT EXISTS transclusion_id UUID",
        "ALTER TABLE tasks ADD COLUMN IF NOT EXISTS epic_id UUID",
        "ALTER TABLE tasks ADD COLUMN IF NOT EXISTS priority TEXT NOT NULL DEFAULT 'medium'",
        "ALTER TABLE tasks ADD COLUMN IF NOT EXISTS owner TEXT",
        "ALTER TABLE tasks ADD COLUMN IF NOT EXISTS labels TEXT[] NOT NULL DEFAULT '{}'",
        "ALTER TABLE links ADD COLUMN IF NOT EXISTS is_xanadu BOOLEAN NOT NULL DEFAULT false",
        "ALTER TABLE links ADD COLUMN IF NOT EXISTS transclusion_id UUID",
        "UPDATE notes SET updated_at = created_at WHERE updated_at IS NULL",
        "
        CREATE INDEX IF NOT EXISTS idx_events_workspace_created_at
        ON events (workspace, created_at DESC)
        ",
        "
        CREATE INDEX IF NOT EXISTS idx_tasks_workspace_status_created_at
        ON tasks (workspace, status, created_at DESC)
        ",
        "
        CREATE INDEX IF NOT EXISTS idx_tasks_workspace_epic_id_created_at
        ON tasks (workspace, epic_id, created_at DESC)
        ",
        "
        CREATE INDEX IF NOT EXISTS idx_tasks_workspace_priority_created_at
        ON tasks (workspace, priority, created_at DESC)
        ",
        "
        CREATE INDEX IF NOT EXISTS idx_tasks_workspace_owner_created_at
        ON tasks (workspace, owner, created_at DESC)
        ",
        "
        CREATE INDEX IF NOT EXISTS idx_tasks_labels_gin
        ON tasks USING GIN (labels)
        ",
        "
        CREATE INDEX IF NOT EXISTS idx_task_claims_task_id_expires_at
        ON task_claims (task_id, expires_at DESC)
        ",
        "
        CREATE INDEX IF NOT EXISTS idx_task_dependencies_task_id
        ON task_dependencies (task_id)
        ",
        "
        CREATE INDEX IF NOT EXISTS idx_task_dependencies_depends_on_task_id
        ON task_dependencies (depends_on_task_id)
        ",
        "
        CREATE INDEX IF NOT EXISTS idx_notes_transclusion_id
        ON notes (transclusion_id)
        ",
        "
        CREATE INDEX IF NOT EXISTS idx_tasks_transclusion_id
        ON tasks (transclusion_id)
        ",
        "
        CREATE INDEX IF NOT EXISTS idx_epics_workspace_created_at
        ON epics (workspace, created_at DESC)
        ",
    ]
}

pub(crate) async fn append_event(
    tx: &mut Transaction<'_, Postgres>,
    workspace: &str,
    actor: &str,
    kind: EventKind,
    payload: &Value,
    created_at: DateTime<Utc>,
) -> ServerResult<Uuid> {
    let event_id = Uuid::new_v4();
    sqlx::query(
        "
        INSERT INTO events (event_id, workspace, actor, kind, payload, created_at)
        VALUES ($1, $2, $3, $4, $5, $6)
        ",
    )
    .bind(event_id)
    .bind(workspace)
    .bind(actor)
    .bind(event_kind_name(kind))
    .bind(payload.clone())
    .bind(created_at)
    .execute(&mut **tx)
    .await?;
    Ok(event_id)
}

pub(crate) fn unique_task_ids(task_ids: &[Uuid]) -> Vec<Uuid> {
    let mut unique_ids = Vec::new();
    for task_id in task_ids {
        if !unique_ids.contains(task_id) {
            unique_ids.push(*task_id);
        }
    }
    unique_ids
}

pub(crate) async fn fetch_event_rows_for_workspace(
    pool: &PgPool,
    workspace: &str,
    limit: i64,
) -> ServerResult<Vec<EventRow>> {
    query_as(
        "
        SELECT event_id, workspace, actor, kind, payload, created_at
        FROM events
        WHERE workspace = $1
        ORDER BY created_at DESC
        LIMIT $2
        ",
    )
    .bind(workspace)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(Into::into)
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
        SELECT claim_id, task_id, workspace, actor, event_id, claimed_at, expires_at
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

pub(crate) async fn fetch_active_claim_tx(
    tx: &mut Transaction<'_, Postgres>,
    task_id: Uuid,
) -> ServerResult<Option<ClaimRow>> {
    query_as(
        "
        SELECT claim_id, task_id, workspace, actor, event_id, claimed_at, expires_at
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
) -> ServerResult<()> {
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

    Ok(())
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

#[derive(Debug, Clone, Copy, Default)]
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
) -> ServerResult<Vec<TaskRow>> {
    if let Some(filter_value) = filters.status {
        if !matches!(filter_value, "open" | "claimed" | "completed") {
            return Err(ThreadplaneServerError::bad_request(format!(
                "unsupported task status filter {filter_value}"
            )));
        }
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
        query.push(" AND ");
        query.push_bind(selected_label);
        query.push(" = ANY(labels)");
    }

    query.push(" ORDER BY created_at DESC");

    let rows = query.build_query_as::<TaskRow>().fetch_all(pool).await?;

    if filters.ready_only {
        let mut ready_rows = Vec::new();
        for row in rows {
            if task_is_ready(pool, row.task_id).await? {
                ready_rows.push(row);
            }
        }
        sort_task_rows_for_queue(&mut ready_rows);
        return Ok(ready_rows);
    }

    Ok(rows)
}

pub(crate) async fn build_task_list_entries(
    pool: &PgPool,
    tasks: Vec<TaskRow>,
) -> ServerResult<Vec<TaskListEntry>> {
    let mut entries = Vec::with_capacity(tasks.len());
    for task in tasks {
        entries.push(build_task_list_entry(pool, task).await?);
    }
    Ok(entries)
}

pub(crate) async fn build_task_list_entry(
    pool: &PgPool,
    task: TaskRow,
) -> ServerResult<TaskListEntry> {
    Ok(TaskListEntry {
        active_claim: fetch_active_claim(pool, task.task_id)
            .await?
            .map(TaskClaimRecord::from),
        dependencies: fetch_direct_dependencies(pool, task.task_id).await?,
        dependents: fetch_direct_dependents(pool, task.task_id).await?,
        epic: fetch_epic_for_task(pool, &task).await?,
        ready: task_is_ready(pool, task.task_id).await?,
        task: task.into(),
    })
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
        Some(EntityRef::Epic(_)) => Err(ThreadplaneServerError::bad_request(format!(
            "epic refs are not textual entities: {entity_ref}"
        ))),
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

pub(crate) fn event_kind_name(kind: EventKind) -> String {
    kind.to_string()
}

pub(crate) fn parse_event_kind(value: &str) -> EventKind {
    EventKind::from_str(value).unwrap_or(EventKind::NoteRecorded)
}

#[derive(Debug, FromRow)]
pub(crate) struct EventRow {
    event_id: Uuid,
    workspace: String,
    actor: String,
    kind: String,
    payload: Value,
    created_at: DateTime<Utc>,
}

#[derive(Debug, FromRow, Clone)]
pub(crate) struct EpicRow {
    epic_id: Uuid,
    event_id: Uuid,
    workspace: String,
    author: String,
    title: String,
    body: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, FromRow, Clone)]
pub(crate) struct NoteRow {
    pub(crate) note_id: Uuid,
    event_id: Uuid,
    workspace: String,
    author: String,
    title: String,
    body: String,
    pub(crate) transclusion_id: Option<Uuid>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, FromRow, Clone)]
pub(crate) struct TaskRow {
    pub(crate) task_id: Uuid,
    event_id: Uuid,
    workspace: String,
    author: String,
    title: String,
    details: String,
    pub(crate) status: String,
    pub(crate) epic_id: Option<Uuid>,
    priority: String,
    owner: Option<String>,
    labels: Vec<String>,
    pub(crate) transclusion_id: Option<Uuid>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, FromRow)]
pub(crate) struct ClaimRow {
    pub(crate) claim_id: Uuid,
    task_id: Uuid,
    workspace: String,
    pub(crate) actor: String,
    event_id: Uuid,
    claimed_at: DateTime<Utc>,
    pub(crate) expires_at: DateTime<Utc>,
}

#[derive(Debug, FromRow)]
struct TransclusionGroupRow {
    title: String,
    content: String,
    updated_at: DateTime<Utc>,
}

pub(crate) enum TextEntityRow {
    Note(NoteRow),
    Task(TaskRow),
}

impl TextEntityRow {
    pub(crate) const fn transclusion_id(&self) -> Option<Uuid> {
        match self {
            Self::Note(note) => note.transclusion_id,
            Self::Task(task) => task.transclusion_id,
        }
    }

    pub(crate) fn title(&self) -> &str {
        match self {
            Self::Note(note) => &note.title,
            Self::Task(task) => &task.title,
        }
    }

    pub(crate) fn content(&self) -> &str {
        match self {
            Self::Note(note) => &note.body,
            Self::Task(task) => &task.details,
        }
    }
}

impl From<EventRow> for EventRecord {
    #[inline]
    fn from(value: EventRow) -> Self {
        Self {
            event_id: value.event_id,
            workspace: value.workspace,
            actor: value.actor,
            kind: parse_event_kind(&value.kind),
            payload: value.payload,
            created_at: value.created_at.to_rfc3339(),
        }
    }
}

impl From<EpicRow> for EpicRecord {
    #[inline]
    fn from(value: EpicRow) -> Self {
        Self {
            epic_id: value.epic_id,
            entity_ref: epic_entity_ref(value.epic_id),
            event_id: value.event_id,
            workspace: value.workspace,
            author: value.author,
            title: value.title,
            body: value.body,
            created_at: value.created_at.to_rfc3339(),
            updated_at: value.updated_at.to_rfc3339(),
        }
    }
}

impl From<NoteRow> for threadplane_core::NoteRecord {
    #[inline]
    fn from(value: NoteRow) -> Self {
        Self {
            note_id: value.note_id,
            entity_ref: note_entity_ref(value.note_id),
            event_id: value.event_id,
            workspace: value.workspace,
            author: value.author,
            title: value.title,
            body: value.body,
            transclusion_id: value.transclusion_id,
            created_at: value.created_at.to_rfc3339(),
            updated_at: value.updated_at.to_rfc3339(),
        }
    }
}

impl From<TaskRow> for TaskRecord {
    #[inline]
    fn from(value: TaskRow) -> Self {
        let metadata = task_metadata_from_row(&value);
        Self {
            task_id: value.task_id,
            entity_ref: task_entity_ref(value.task_id),
            event_id: value.event_id,
            workspace: value.workspace,
            author: value.author,
            title: value.title,
            details: value.details,
            status: value.status,
            epic_id: value.epic_id,
            metadata,
            transclusion_id: value.transclusion_id,
            created_at: value.created_at.to_rfc3339(),
            updated_at: value.updated_at.to_rfc3339(),
        }
    }
}

impl From<TaskRow> for TaskSummary {
    #[inline]
    fn from(value: TaskRow) -> Self {
        let metadata = task_metadata_from_row(&value);
        Self {
            task_id: value.task_id,
            entity_ref: task_entity_ref(value.task_id),
            workspace: value.workspace,
            title: value.title,
            details: value.details,
            status: value.status,
            epic_id: value.epic_id,
            author: value.author,
            metadata,
            transclusion_id: value.transclusion_id,
            created_at: value.created_at.to_rfc3339(),
            updated_at: value.updated_at.to_rfc3339(),
        }
    }
}

fn task_metadata_from_row(value: &TaskRow) -> TaskMetadata {
    TaskMetadata {
        labels: normalize_task_labels(value.labels.clone()),
        owner: normalize_task_owner(value.owner.clone()),
        priority: parse_task_priority(&value.priority),
    }
}

fn parse_task_priority(value: &str) -> TaskPriority {
    value.parse().unwrap_or_default()
}

fn sort_task_rows_for_queue(rows: &mut [TaskRow]) {
    rows.sort_by(|left, right| {
        let left_priority = task_priority_rank(parse_task_priority(&left.priority));
        let right_priority = task_priority_rank(parse_task_priority(&right.priority));

        right_priority
            .cmp(&left_priority)
            .then_with(|| right.updated_at.cmp(&left.updated_at))
            .then_with(|| right.created_at.cmp(&left.created_at))
    });
}

pub(crate) const fn task_priority_rank(priority: TaskPriority) -> u8 {
    match priority {
        TaskPriority::Low => 0,
        TaskPriority::Medium => 1,
        TaskPriority::High => 2,
        TaskPriority::Urgent => 3,
    }
}

impl From<ClaimRow> for TaskClaimRecord {
    #[inline]
    fn from(value: ClaimRow) -> Self {
        Self {
            claim_id: value.claim_id,
            task_id: value.task_id,
            workspace: value.workspace,
            actor: value.actor,
            event_id: value.event_id,
            claimed_at: value.claimed_at.to_rfc3339(),
            expires_at: value.expires_at.to_rfc3339(),
        }
    }
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
