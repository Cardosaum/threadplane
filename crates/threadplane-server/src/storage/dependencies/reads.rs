use super::*;

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
