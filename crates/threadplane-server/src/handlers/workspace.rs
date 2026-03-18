#![expect(
    clippy::redundant_pub_crate,
    reason = "Workspace handlers are crate-local endpoints with explicit visibility."
)]
#![allow(
    clippy::wildcard_imports,
    reason = "The workspace handler submodule intentionally builds on the handler prelude."
)]

use super::*;

fn ensure_path_workspace_matches(
    path_workspace: &str,
    request_workspace: &str,
) -> ServerResult<()> {
    if request_workspace == path_workspace {
        return Ok(());
    }

    Err(ThreadplaneServerError::bad_request(
        "request workspace must match the path workspace",
    ))
}

pub(crate) async fn update_workspace_policy(
    State(state): State<AppState>,
    Path(WorkspacePath { workspace }): Path<WorkspacePath>,
    Json(request): Json<UpdateWorkspacePolicyRequest>,
) -> AppResult<WorkspacePolicy> {
    ensure_path_workspace_matches(&workspace, &request.workspace)?;

    require_workspace_admin()
        .pool(state.pool())
        .bootstrap(state.bootstrap())
        .workspace(&workspace)
        .actor(&request.actor)
        .call()
        .await?;
    let data = upsert_workspace_policy(
        state.pool(),
        &WorkspacePolicy {
            auth: request.auth,
            priorities: request.priorities,
            workspace,
        },
    )
    .await?;
    Ok(success(data))
}

pub(crate) async fn grant_workspace_membership(
    State(state): State<AppState>,
    Path(WorkspacePath { workspace }): Path<WorkspacePath>,
    Json(request): Json<GrantWorkspaceMembershipRequest>,
) -> AppResult<WorkspaceMembership> {
    ensure_path_workspace_matches(&workspace, &request.workspace)?;

    require_workspace_admin()
        .pool(state.pool())
        .bootstrap(state.bootstrap())
        .workspace(&workspace)
        .actor(&request.actor)
        .call()
        .await?;
    let data = upsert_workspace_membership(
        state.pool(),
        &WorkspaceMembership {
            actor_id: request.member_actor_id,
            role: request.role,
            workspace,
        },
    )
    .await?;
    Ok(success(data))
}

pub(crate) async fn add_workspace_public_key(
    State(state): State<AppState>,
    Path(WorkspacePath { workspace }): Path<WorkspacePath>,
    Json(request): Json<AddWorkspacePublicKeyRequest>,
) -> AppResult<ActorPublicKey> {
    ensure_path_workspace_matches(&workspace, &request.workspace)?;

    require_workspace_admin()
        .pool(state.pool())
        .bootstrap(state.bootstrap())
        .workspace(&workspace)
        .actor(&request.actor)
        .call()
        .await?;
    let data = upsert_actor_public_key(
        state.pool(),
        &workspace,
        &ActorPublicKey {
            actor_id: request.member_actor_id,
            algorithm: request.algorithm,
            key_id: request.key_id,
            public_key: request.public_key,
        },
    )
    .await?;
    Ok(success(data))
}
