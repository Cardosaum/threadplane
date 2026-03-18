use super::*;

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
            allowed_algorithms: super::keys::parse_public_key_algorithms(
                &policy_row.allowed_algorithms,
            )?,
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
            .map(super::keys::serialize_public_key_algorithm)
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

pub(super) fn normalize_priority_name(value: &str) -> String {
    TaskPriority::new(value)
        .map(|priority| priority.to_string())
        .unwrap_or_default()
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
