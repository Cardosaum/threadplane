use super::*;

#[builder]
pub(crate) async fn ensure_supported_task_priority(
    bootstrap: &WorkspaceGovernanceBootstrap,
    pool: &PgPool,
    priority: &TaskPriority,
    workspace: &str,
) -> ServerResult<()> {
    ensure_workspace_policy(pool, bootstrap, workspace).await?;
    if workspace_supports_priority(pool, workspace, priority).await? {
        return Ok(());
    }

    Err(ThreadplaneServerError::bad_request(format!(
        "unsupported task priority `{priority}` in workspace {workspace}"
    )))
}

pub(crate) async fn project_task_record(
    graph: &Graph,
    pool: &PgPool,
    record: &TaskRecord,
) -> ServerResult<()> {
    project_task_supporting_entities(graph, pool, record).await?;
    project_task(graph, record).await.map_err(|error| {
        error!(?error, task_id = %record.task_id, "failed to project task");
        ThreadplaneServerError::internal(error)
    })
}
