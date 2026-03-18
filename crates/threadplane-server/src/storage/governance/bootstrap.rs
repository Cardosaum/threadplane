use super::*;

pub(crate) async fn ensure_workspace_governance(
    pool: &PgPool,
    workspace: &str,
    bootstrap: &WorkspaceGovernanceBootstrap,
) -> ServerResult<WorkspacePolicy> {
    let mut tx = pool.begin().await?;
    let policy = bootstrap.policy_for_workspace(workspace);
    insert_workspace_policy_if_missing(&mut tx, &policy).await?;
    insert_workspace_priorities_if_missing(&mut tx, workspace, &policy.priorities).await?;
    insert_workspace_memberships_if_missing(
        &mut tx,
        &bootstrap.memberships_for_workspace(workspace),
    )
    .await?;
    insert_actor_public_keys_if_missing(&mut tx, workspace, &bootstrap.public_keys()).await?;
    tx.commit().await?;

    super::fetch_workspace_policy(pool, workspace).await
}

async fn insert_workspace_policy_if_missing(
    tx: &mut Transaction<'_, Postgres>,
    policy: &WorkspacePolicy,
) -> ServerResult<()> {
    sqlx::query(
        "
        INSERT INTO workspace_policies (
            workspace,
            default_priority,
            allowed_algorithms,
            challenge_ttl_seconds,
            signed_commands_required
        )
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT (workspace) DO NOTHING
        ",
    )
    .bind(&policy.workspace)
    .bind(super::policy::normalize_priority_name(
        &policy.priorities.default_priority,
    ))
    .bind(
        policy
            .auth
            .allowed_algorithms
            .iter()
            .copied()
            .map(super::keys::serialize_public_key_algorithm)
            .collect::<Vec<_>>(),
    )
    .bind(
        i32::try_from(policy.auth.challenge_ttl_seconds)
            .map_err(ThreadplaneServerError::internal)?,
    )
    .bind(policy.auth.signed_commands_required)
    .execute(&mut **tx)
    .await?;

    Ok(())
}

async fn insert_workspace_priorities_if_missing(
    tx: &mut Transaction<'_, Postgres>,
    workspace: &str,
    priorities: &WorkspacePriorityPolicy,
) -> ServerResult<()> {
    for priority in &priorities.priorities {
        sqlx::query(
            "
            INSERT INTO workspace_priorities (workspace, name, rank, description)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (workspace, name) DO NOTHING
            ",
        )
        .bind(workspace)
        .bind(super::policy::normalize_priority_name(&priority.name))
        .bind(i32::from(priority.rank))
        .bind(priority.description.clone())
        .execute(&mut **tx)
        .await?;
    }

    Ok(())
}

async fn insert_workspace_memberships_if_missing(
    tx: &mut Transaction<'_, Postgres>,
    memberships: &[WorkspaceMembership],
) -> ServerResult<()> {
    for membership in memberships {
        sqlx::query(
            "
            INSERT INTO workspace_memberships (workspace, actor_id, role)
            VALUES ($1, $2, $3)
            ON CONFLICT (workspace, actor_id) DO NOTHING
            ",
        )
        .bind(&membership.workspace)
        .bind(&membership.actor_id)
        .bind(membership.role.to_string())
        .execute(&mut **tx)
        .await?;
    }

    Ok(())
}

async fn insert_actor_public_keys_if_missing(
    tx: &mut Transaction<'_, Postgres>,
    workspace: &str,
    public_keys: &[ActorPublicKey],
) -> ServerResult<()> {
    for key in public_keys {
        sqlx::query(
            "
            INSERT INTO actor_public_keys (workspace, actor_id, key_id, algorithm, public_key)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (workspace, actor_id, key_id) DO NOTHING
            ",
        )
        .bind(workspace)
        .bind(&key.actor_id)
        .bind(&key.key_id)
        .bind(key.algorithm.to_string())
        .bind(&key.public_key)
        .execute(&mut **tx)
        .await?;
    }

    Ok(())
}
