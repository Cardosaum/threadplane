use super::*;

#[derive(Debug, FromRow)]
struct WorkspaceMembershipRow {
    workspace: String,
    actor_id: String,
    role: String,
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

fn parse_workspace_role(value: &str) -> ServerResult<WorkspaceRole> {
    value.parse().map_err(|_error| {
        ThreadplaneServerError::internal(format!("unsupported stored workspace role {value}"))
    })
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
