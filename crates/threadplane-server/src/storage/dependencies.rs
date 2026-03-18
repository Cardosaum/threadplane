#![expect(
    clippy::redundant_pub_crate,
    reason = "Dependency persistence is shared only inside this crate."
)]
#![allow(
    clippy::wildcard_imports,
    reason = "The dependency submodule intentionally builds on the storage prelude."
)]

use super::*;

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
