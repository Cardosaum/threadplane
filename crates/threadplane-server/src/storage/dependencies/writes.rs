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

async fn dependency_would_create_cycle(
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
