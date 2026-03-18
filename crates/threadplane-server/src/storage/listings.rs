#![expect(
    clippy::redundant_pub_crate,
    reason = "Listing queries are shared only inside this crate."
)]
#![allow(
    clippy::wildcard_imports,
    reason = "The listings submodule intentionally builds on the storage prelude."
)]

use super::*;

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

    query
        .build_query_as::<TaskRow>()
        .fetch_all(pool)
        .await
        .map_err(Into::into)
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
