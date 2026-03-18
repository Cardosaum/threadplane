use super::*;

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
