#![allow(
    clippy::wildcard_imports,
    reason = "Read handlers use the parent handler module as their import boundary."
)]

use super::super::*;

pub(crate) async fn show_workspace_policy(
    State(bootstrap): State<WorkspaceGovernanceBootstrap>,
    State(pool): State<PgPool>,
    Path(WorkspacePath { workspace }): Path<WorkspacePath>,
) -> AppResult<WorkspacePolicy> {
    Ok(success(
        ensure_workspace_policy(&pool, &bootstrap, &workspace).await?,
    ))
}

pub(crate) async fn list_workspace_memberships(
    State(bootstrap): State<WorkspaceGovernanceBootstrap>,
    State(pool): State<PgPool>,
    Path(WorkspacePath { workspace }): Path<WorkspacePath>,
) -> AppResult<Vec<WorkspaceMembership>> {
    ensure_workspace_policy(&pool, &bootstrap, &workspace).await?;
    Ok(success(
        fetch_workspace_memberships(&pool, &workspace).await?,
    ))
}

pub(crate) async fn list_workspace_public_keys(
    State(bootstrap): State<WorkspaceGovernanceBootstrap>,
    State(pool): State<PgPool>,
    Path(WorkspacePath { workspace }): Path<WorkspacePath>,
    Query(query): Query<WorkspaceKeysQuery>,
) -> AppResult<Vec<ActorPublicKey>> {
    ensure_workspace_policy(&pool, &bootstrap, &workspace).await?;
    Ok(success(
        fetch_actor_public_keys(&pool, &workspace, query.actor_id.as_deref()).await?,
    ))
}
