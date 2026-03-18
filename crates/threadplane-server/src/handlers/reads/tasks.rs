#![allow(
    clippy::wildcard_imports,
    reason = "Read handlers use the parent handler module as their import boundary."
)]

use super::super::*;

async fn build_optional_task_entry(
    pool: &PgPool,
    row: Option<TaskRow>,
) -> ServerResult<Option<TaskListEntry>> {
    if let Some(task) = row {
        let mut entries = build_task_list_entries(pool, vec![task]).await?;
        Ok(entries.pop())
    } else {
        Ok(None)
    }
}

pub(crate) async fn list_tasks(
    State(bootstrap): State<WorkspaceGovernanceBootstrap>,
    State(pool): State<PgPool>,
    Path(WorkspacePath { workspace }): Path<WorkspacePath>,
    Query(query): Query<TaskListQuery>,
) -> AppResult<Vec<TaskListEntry>> {
    ensure_workspace_policy(&pool, &bootstrap, &workspace).await?;
    let rows = fetch_tasks_for_listing(
        &pool,
        &workspace,
        task_selection_filters(&query),
        Some(normalized_list_limit(query.limit)),
    )
    .await?;
    Ok(success(build_task_list_entries(&pool, rows).await?))
}

pub(crate) async fn next_task(
    State(bootstrap): State<WorkspaceGovernanceBootstrap>,
    State(pool): State<PgPool>,
    Path(WorkspacePath { workspace }): Path<WorkspacePath>,
    Query(query): Query<TaskListQuery>,
) -> AppResult<Option<TaskListEntry>> {
    ensure_workspace_policy(&pool, &bootstrap, &workspace).await?;
    let row = fetch_tasks_for_listing(&pool, &workspace, task_next_filters(&query), Some(1))
        .await?
        .into_iter()
        .next();
    Ok(success(build_optional_task_entry(&pool, row).await?))
}

pub(crate) async fn list_open_tasks(
    State(bootstrap): State<WorkspaceGovernanceBootstrap>,
    State(pool): State<PgPool>,
    Path(WorkspacePath { workspace }): Path<WorkspacePath>,
) -> AppResult<Vec<TaskListEntry>> {
    ensure_workspace_policy(&pool, &bootstrap, &workspace).await?;
    let rows = fetch_tasks_for_listing(
        &pool,
        &workspace,
        TaskListFilters {
            ready_only: false,
            status: Some("open"),
            ..TaskListFilters::default()
        },
        None,
    )
    .await?;
    Ok(success(build_task_list_entries(&pool, rows).await?))
}

pub(crate) async fn show_task(
    State(pool): State<PgPool>,
    Path(TaskPath { task_id }): Path<TaskPath>,
) -> AppResult<TaskRecord> {
    Ok(success(TaskRecord::from(
        fetch_task_by_id(&pool, task_id).await?,
    )))
}

pub(crate) async fn task_context(
    State(graph): State<Arc<Graph>>,
    State(pool): State<PgPool>,
    Path(TaskPath { task_id }): Path<TaskPath>,
) -> AppResult<TaskContext> {
    let task = fetch_task_by_id(&pool, task_id).await?;
    let active_claim = fetch_active_claim(&pool, task_id)
        .await?
        .map(TaskClaimRecord::from);
    let epic = fetch_epic_for_task(&pool, &task).await?;
    let dependencies = fetch_direct_dependencies(&pool, task_id).await?;
    let dependents = fetch_direct_dependents(&pool, task_id).await?;
    let relations =
        fetch_entity_relations(graph.as_ref(), &threadplane_core::task_entity_ref(task_id))
            .await
            .map_err(ThreadplaneServerError::internal)?;

    Ok(success(TaskContext {
        task: task.clone().into(),
        active_claim,
        dependencies,
        dependents,
        epic,
        ready: task_is_ready(&pool, task_id).await?,
        relations,
    }))
}

pub(crate) async fn task_dag(
    State(pool): State<PgPool>,
    Path(TaskPath { task_id }): Path<TaskPath>,
) -> AppResult<TaskDag> {
    let task = fetch_task_by_id(&pool, task_id).await?;
    Ok(success(TaskDag {
        task: task.clone().into(),
        epic: fetch_epic_for_task(&pool, &task).await?,
        ready: task_is_ready(&pool, task_id).await?,
        dependencies: fetch_dependency_chain(&pool, task_id).await?,
        dependents: fetch_dependent_chain(&pool, task_id).await?,
    }))
}
