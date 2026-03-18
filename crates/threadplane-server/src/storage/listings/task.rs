use super::*;

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
