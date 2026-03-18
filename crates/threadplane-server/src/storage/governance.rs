#![expect(
    clippy::redundant_pub_crate,
    reason = "Governance persistence is shared only inside this crate."
)]
#![allow(
    clippy::wildcard_imports,
    reason = "The storage governance module intentionally builds on the storage prelude."
)]

use super::*;
use threadplane_core::{
    validate_workspace_policy, ActorPublicKey, PublicKeyAlgorithm, TaskPriority,
    WorkspaceAuthPolicy, WorkspaceMembership, WorkspacePolicy, WorkspacePriority,
    WorkspacePriorityPolicy, WorkspaceRole,
};

#[derive(Debug, FromRow)]
struct WorkspacePolicyRow {
    workspace: String,
    default_priority: String,
    allowed_algorithms: Vec<String>,
    challenge_ttl_seconds: i32,
    signed_commands_required: bool,
}

#[derive(Debug, FromRow)]
struct WorkspacePriorityRow {
    name: String,
    rank: i32,
    description: Option<String>,
}

#[derive(Debug, FromRow)]
struct WorkspaceMembershipRow {
    workspace: String,
    actor_id: String,
    role: String,
}

#[derive(Debug, FromRow)]
struct ActorPublicKeyRow {
    actor_id: String,
    algorithm: String,
    key_id: String,
    public_key: String,
}

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

    fetch_workspace_policy(pool, workspace).await
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
    .bind(policy.priorities.default_priority.clone())
    .bind(
        policy
            .auth
            .allowed_algorithms
            .iter()
            .copied()
            .map(serialize_public_key_algorithm)
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
        .bind(normalize_priority_name(&priority.name))
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

pub(crate) async fn fetch_workspace_policy(
    pool: &PgPool,
    workspace: &str,
) -> ServerResult<WorkspacePolicy> {
    let policy_row: WorkspacePolicyRow = query_as(
        "
        SELECT
            workspace,
            default_priority,
            allowed_algorithms,
            challenge_ttl_seconds,
            signed_commands_required
        FROM workspace_policies
        WHERE workspace = $1
        ",
    )
    .bind(workspace)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| ThreadplaneServerError::not_found(format!("workspace policy {workspace}")))?;

    let priority_rows: Vec<WorkspacePriorityRow> = query_as(
        "
        SELECT name, rank, description
        FROM workspace_priorities
        WHERE workspace = $1
        ORDER BY rank DESC, name ASC
        ",
    )
    .bind(workspace)
    .fetch_all(pool)
    .await?;

    let policy = WorkspacePolicy {
        auth: WorkspaceAuthPolicy {
            allowed_algorithms: parse_public_key_algorithms(&policy_row.allowed_algorithms)?,
            challenge_ttl_seconds: u32::try_from(policy_row.challenge_ttl_seconds)
                .map_err(ThreadplaneServerError::internal)?,
            signed_commands_required: policy_row.signed_commands_required,
        },
        priorities: WorkspacePriorityPolicy {
            default_priority: normalize_priority_name(&policy_row.default_priority),
            priorities: priority_rows
                .into_iter()
                .map(TryInto::try_into)
                .collect::<ServerResult<Vec<_>>>()?,
        },
        workspace: policy_row.workspace,
    };
    validate_workspace_policy(&policy).map_err(|error| {
        ThreadplaneServerError::internal(format!("invalid stored workspace policy: {error}"))
    })?;
    Ok(policy)
}

pub(crate) async fn upsert_workspace_policy(
    pool: &PgPool,
    policy: &WorkspacePolicy,
) -> ServerResult<WorkspacePolicy> {
    validate_workspace_policy(policy)
        .map_err(|error| ThreadplaneServerError::bad_request(error.to_string()))?;

    let mut tx = pool.begin().await?;
    sqlx::query(
        "
        INSERT INTO workspace_policies (
            workspace,
            default_priority,
            allowed_algorithms,
            challenge_ttl_seconds,
            signed_commands_required,
            updated_at
        )
        VALUES ($1, $2, $3, $4, $5, now())
        ON CONFLICT (workspace) DO UPDATE
        SET default_priority = excluded.default_priority,
            allowed_algorithms = excluded.allowed_algorithms,
            challenge_ttl_seconds = excluded.challenge_ttl_seconds,
            signed_commands_required = excluded.signed_commands_required,
            updated_at = now()
        ",
    )
    .bind(&policy.workspace)
    .bind(normalize_priority_name(&policy.priorities.default_priority))
    .bind(
        policy
            .auth
            .allowed_algorithms
            .iter()
            .copied()
            .map(serialize_public_key_algorithm)
            .collect::<Vec<_>>(),
    )
    .bind(
        i32::try_from(policy.auth.challenge_ttl_seconds)
            .map_err(ThreadplaneServerError::internal)?,
    )
    .bind(policy.auth.signed_commands_required)
    .execute(&mut *tx)
    .await?;

    sqlx::query("DELETE FROM workspace_priorities WHERE workspace = $1")
        .bind(&policy.workspace)
        .execute(&mut *tx)
        .await?;

    for priority in &policy.priorities.priorities {
        sqlx::query(
            "
            INSERT INTO workspace_priorities (workspace, name, rank, description)
            VALUES ($1, $2, $3, $4)
            ",
        )
        .bind(&policy.workspace)
        .bind(normalize_priority_name(&priority.name))
        .bind(i32::from(priority.rank))
        .bind(priority.description.clone())
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    fetch_workspace_policy(pool, &policy.workspace).await
}

pub(crate) async fn fetch_workspace_memberships(
    pool: &PgPool,
    workspace: &str,
) -> ServerResult<Vec<WorkspaceMembership>> {
    let rows: Vec<WorkspaceMembershipRow> = query_as(
        "
        SELECT workspace, actor_id, role
        FROM workspace_memberships
        WHERE workspace = $1
        ORDER BY role ASC, actor_id ASC
        ",
    )
    .bind(workspace)
    .fetch_all(pool)
    .await?;
    rows.into_iter().map(TryInto::try_into).collect()
}

pub(crate) async fn upsert_workspace_membership(
    pool: &PgPool,
    membership: &WorkspaceMembership,
) -> ServerResult<WorkspaceMembership> {
    sqlx::query(
        "
        INSERT INTO workspace_memberships (workspace, actor_id, role, updated_at)
        VALUES ($1, $2, $3, now())
        ON CONFLICT (workspace, actor_id) DO UPDATE
        SET role = excluded.role,
            updated_at = now()
        ",
    )
    .bind(&membership.workspace)
    .bind(&membership.actor_id)
    .bind(membership.role.to_string())
    .execute(pool)
    .await?;
    Ok(membership.clone())
}

pub(crate) async fn fetch_workspace_role(
    pool: &PgPool,
    workspace: &str,
    actor_id: &str,
) -> ServerResult<Option<WorkspaceRole>> {
    let role = sqlx::query_scalar::<_, String>(
        "
        SELECT role
        FROM workspace_memberships
        WHERE workspace = $1
          AND actor_id = $2
        ",
    )
    .bind(workspace)
    .bind(actor_id)
    .fetch_optional(pool)
    .await?;

    role.as_deref().map(parse_workspace_role).transpose()
}

pub(crate) async fn fetch_actor_public_keys(
    pool: &PgPool,
    workspace: &str,
    actor_id: Option<&str>,
) -> ServerResult<Vec<ActorPublicKey>> {
    let rows = if let Some(selected_actor_id) = actor_id {
        query_as::<_, ActorPublicKeyRow>(
            "
            SELECT actor_id, algorithm, key_id, public_key
            FROM actor_public_keys
            WHERE workspace = $1
              AND actor_id = $2
            ORDER BY actor_id ASC, key_id ASC
            ",
        )
        .bind(workspace)
        .bind(selected_actor_id)
        .fetch_all(pool)
        .await?
    } else {
        query_as::<_, ActorPublicKeyRow>(
            "
            SELECT actor_id, algorithm, key_id, public_key
            FROM actor_public_keys
            WHERE workspace = $1
            ORDER BY actor_id ASC, key_id ASC
            ",
        )
        .bind(workspace)
        .fetch_all(pool)
        .await?
    };

    rows.into_iter().map(TryInto::try_into).collect()
}

pub(crate) async fn upsert_actor_public_key(
    pool: &PgPool,
    workspace: &str,
    key: &ActorPublicKey,
) -> ServerResult<ActorPublicKey> {
    sqlx::query(
        "
        INSERT INTO actor_public_keys (
            workspace,
            actor_id,
            key_id,
            algorithm,
            public_key,
            updated_at
        )
        VALUES ($1, $2, $3, $4, $5, now())
        ON CONFLICT (workspace, actor_id, key_id) DO UPDATE
        SET algorithm = excluded.algorithm,
            public_key = excluded.public_key,
            updated_at = now()
        ",
    )
    .bind(workspace)
    .bind(&key.actor_id)
    .bind(&key.key_id)
    .bind(key.algorithm.to_string())
    .bind(&key.public_key)
    .execute(pool)
    .await?;
    Ok(key.clone())
}

pub(crate) async fn require_workspace_role(
    pool: &PgPool,
    workspace: &str,
    actor_id: &str,
    predicate: impl FnOnce(WorkspaceRole) -> bool,
    capability: &str,
) -> ServerResult<WorkspaceRole> {
    let role = fetch_workspace_role(pool, workspace, actor_id)
        .await?
        .ok_or_else(|| {
            ThreadplaneServerError::forbidden(format!(
                "actor {actor_id} is not a member of workspace {workspace}"
            ))
        })?;

    if !predicate(role) {
        return Err(ThreadplaneServerError::forbidden(format!(
            "actor {actor_id} lacks permission to {capability} in workspace {workspace}"
        )));
    }

    Ok(role)
}

pub(crate) async fn workspace_supports_priority(
    pool: &PgPool,
    workspace: &str,
    priority: &TaskPriority,
) -> ServerResult<bool> {
    let count = sqlx::query_scalar::<_, i64>(
        "
        SELECT COUNT(*)
        FROM workspace_priorities
        WHERE workspace = $1
          AND name = $2
        ",
    )
    .bind(workspace)
    .bind(priority.as_str())
    .fetch_one(pool)
    .await?;
    Ok(count > 0)
}

fn normalize_priority_name(value: &str) -> String {
    TaskPriority::new(value)
        .map(|priority| priority.to_string())
        .unwrap_or_default()
}

fn parse_public_key_algorithms(values: &[String]) -> ServerResult<Vec<PublicKeyAlgorithm>> {
    values
        .iter()
        .map(|value| parse_public_key_algorithm(value))
        .collect()
}

fn parse_public_key_algorithm(value: &str) -> ServerResult<PublicKeyAlgorithm> {
    value.parse().map_err(|_error| {
        ThreadplaneServerError::internal(format!("unsupported stored public-key algorithm {value}"))
    })
}

fn serialize_public_key_algorithm(value: PublicKeyAlgorithm) -> String {
    match value {
        PublicKeyAlgorithm::Ed25519 => "ed25519".to_owned(),
        PublicKeyAlgorithm::Secp256k1 => "secp256k1".to_owned(),
        PublicKeyAlgorithm::SshEd25519 => "ssh_ed25519".to_owned(),
    }
}

fn parse_workspace_role(value: &str) -> ServerResult<WorkspaceRole> {
    value.parse().map_err(|_error| {
        ThreadplaneServerError::internal(format!("unsupported stored workspace role {value}"))
    })
}

impl TryFrom<WorkspacePriorityRow> for WorkspacePriority {
    type Error = ThreadplaneServerError;

    #[inline]
    fn try_from(value: WorkspacePriorityRow) -> ServerResult<Self> {
        let rank = u16::try_from(value.rank)
            .map_err(|error| ThreadplaneServerError::internal(error.to_string()))?;
        Ok(Self {
            description: value.description,
            name: value.name,
            rank,
        })
    }
}

impl TryFrom<WorkspaceMembershipRow> for WorkspaceMembership {
    type Error = ThreadplaneServerError;

    #[inline]
    fn try_from(value: WorkspaceMembershipRow) -> ServerResult<Self> {
        Ok(Self {
            actor_id: value.actor_id,
            role: parse_workspace_role(&value.role)?,
            workspace: value.workspace,
        })
    }
}

impl TryFrom<ActorPublicKeyRow> for ActorPublicKey {
    type Error = ThreadplaneServerError;

    #[inline]
    fn try_from(value: ActorPublicKeyRow) -> ServerResult<Self> {
        Ok(Self {
            actor_id: value.actor_id,
            algorithm: parse_public_key_algorithm(&value.algorithm)?,
            key_id: value.key_id,
            public_key: value.public_key,
        })
    }
}
