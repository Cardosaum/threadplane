#![expect(
    clippy::arbitrary_source_item_ordering,
    reason = "Persistence helpers are grouped by capability and query workflow."
)]
#![expect(
    clippy::redundant_pub_crate,
    reason = "Persistence helpers are shared only inside this crate."
)]

use alloc::collections::BTreeMap;
use core::str::FromStr as _;

use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use sqlx::{query_as, FromRow, PgPool, Postgres, QueryBuilder, Transaction};
use uuid::Uuid;

use crate::{
    app::WorkspaceGovernanceBootstrap,
    error::{ServerResult, ThreadplaneServerError},
};
use threadplane_core::{
    epic_entity_ref, normalize_task_labels, normalize_task_owner, note_entity_ref,
    parse_entity_ref, task_entity_ref, validate_workspace_policy, ActorPublicKey, EntityRef,
    EntityRecord, EpicRecord, EventKind, EventRecord, NoteRecord, ProjectionStatus,
    PublicKeyAlgorithm, TaskClaimRecord, TaskDependencySummary, TaskListEntry, TaskMetadata,
    TaskPriority, TaskRecord, TaskSummary, WorkspaceAuthPolicy, WorkspaceMembership, WorkspacePolicy,
    WorkspacePriority, WorkspacePriorityPolicy, WorkspaceRole, DEPENDS_ON_RELATION,
};

pub(crate) const NOTE_SELECT: &str = "
    SELECT
        note_id,
        event_id,
        workspace,
        author,
        title,
        body,
        transclusion_id,
        created_at,
        COALESCE(updated_at, created_at) AS updated_at
    FROM notes
";

pub(crate) const EPIC_SELECT: &str = "
    SELECT
        epic_id,
        event_id,
        workspace,
        author,
        title,
        body,
        created_at,
        COALESCE(updated_at, created_at) AS updated_at
    FROM epics
";

pub(crate) const TASK_SELECT: &str = "
    SELECT
        task_id,
        event_id,
        workspace,
        author,
        title,
        details,
        status,
        epic_id,
        priority,
        owner,
        labels,
        transclusion_id,
        created_at,
        COALESCE(updated_at, created_at) AS updated_at
    FROM tasks
";

pub(crate) const CLAIM_SELECT: &str = "
    SELECT
        claim_id,
        task_id,
        workspace,
        actor,
        event_id,
        claimed_at,
        expires_at,
        released_at
    FROM task_claims
";

pub(crate) const LINK_SELECT: &str = "
    SELECT
        link_id,
        event_id,
        workspace,
        actor,
        from_entity_ref,
        to_entity_ref,
        relation,
        is_xanadu,
        transclusion_id,
        created_at
    FROM links
";

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
    .bind(workspace)
    .bind(bootstrap.policy_for_workspace(workspace).priorities.default_priority)
        .bind(
            bootstrap
                .policy_for_workspace(workspace)
                .auth
                .allowed_algorithms
                .iter()
                .copied()
                .map(serialize_public_key_algorithm)
                .collect::<Vec<_>>(),
        )
    .bind(i32::try_from(
        bootstrap
            .policy_for_workspace(workspace)
            .auth
            .challenge_ttl_seconds,
    )
    .map_err(ThreadplaneServerError::internal)?)
    .bind(
        bootstrap
            .policy_for_workspace(workspace)
            .auth
            .signed_commands_required,
    )
    .execute(&mut *tx)
    .await?;

    for priority in &bootstrap.policy_for_workspace(workspace).priorities.priorities {
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
        .execute(&mut *tx)
        .await?;
    }

    for membership in bootstrap.memberships_for_workspace(workspace) {
        sqlx::query(
            "
            INSERT INTO workspace_memberships (workspace, actor_id, role)
            VALUES ($1, $2, $3)
            ON CONFLICT (workspace, actor_id) DO NOTHING
            ",
        )
        .bind(workspace)
        .bind(membership.actor_id)
        .bind(membership.role.to_string())
        .execute(&mut *tx)
        .await?;
    }

    for key in bootstrap.public_keys() {
        sqlx::query(
            "
            INSERT INTO actor_public_keys (workspace, actor_id, key_id, algorithm, public_key)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (workspace, actor_id, key_id) DO NOTHING
            ",
        )
        .bind(workspace)
        .bind(key.actor_id)
        .bind(key.key_id)
        .bind(key.algorithm.to_string())
        .bind(key.public_key)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;

    fetch_workspace_policy(pool, workspace).await
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
    .bind(i32::try_from(policy.auth.challenge_ttl_seconds).map_err(ThreadplaneServerError::internal)?)
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
        .ok_or_else(|| ThreadplaneServerError::forbidden(format!(
            "actor {actor_id} is not a member of workspace {workspace}"
        )))?;

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
    values.iter().map(|value| parse_public_key_algorithm(value)).collect()
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

pub(crate) async fn append_event(
    tx: &mut Transaction<'_, Postgres>,
    workspace: &str,
    actor: &str,
    kind: EventKind,
    payload: &Value,
    created_at: DateTime<Utc>,
) -> ServerResult<Uuid> {
    let event_id = Uuid::new_v4();
    sqlx::query(
        "
        INSERT INTO events (event_id, workspace, actor, kind, payload, created_at)
        VALUES ($1, $2, $3, $4, $5, $6)
        ",
    )
    .bind(event_id)
    .bind(workspace)
    .bind(actor)
    .bind(event_kind_name(kind))
    .bind(payload.clone())
    .bind(created_at)
    .execute(&mut **tx)
    .await?;
    Ok(event_id)
}

pub(crate) fn unique_task_ids(task_ids: &[Uuid]) -> Vec<Uuid> {
    let mut unique_ids = Vec::new();
    for task_id in task_ids {
        if !unique_ids.contains(task_id) {
            unique_ids.push(*task_id);
        }
    }
    unique_ids
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ProjectionCursor {
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) event_id: Uuid,
}

impl ProjectionCursor {
    #[must_use]
    pub(crate) const fn new(created_at: DateTime<Utc>, event_id: Uuid) -> Self {
        Self {
            created_at,
            event_id,
        }
    }
}

pub(crate) async fn fetch_event_rows_for_workspace(
    pool: &PgPool,
    workspace: &str,
    limit: i64,
) -> ServerResult<Vec<EventRow>> {
    query_as(
        "
        SELECT event_id, workspace, actor, kind, payload, created_at
        FROM events
        WHERE workspace = $1
        ORDER BY created_at DESC
        LIMIT $2
        ",
    )
    .bind(workspace)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(Into::into)
}

pub(crate) async fn fetch_event_row_for_workspace(
    pool: &PgPool,
    workspace: &str,
    event_id: Uuid,
) -> ServerResult<EventRow> {
    query_as(
        "
        SELECT event_id, workspace, actor, kind, payload, created_at
        FROM events
        WHERE workspace = $1
          AND event_id = $2
        ",
    )
    .bind(workspace)
    .bind(event_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| ThreadplaneServerError::not_found("event not found"))
}

pub(crate) async fn fetch_event_rows_after_workspace_cursor(
    pool: &PgPool,
    workspace: &str,
    cursor: Option<ProjectionCursor>,
    limit: i64,
) -> ServerResult<Vec<EventRow>> {
    if let Some(current_cursor) = cursor {
        return query_as(
            "
            SELECT event_id, workspace, actor, kind, payload, created_at
            FROM events
            WHERE workspace = $1
              AND (
                    created_at > $2
                 OR (created_at = $2 AND event_id > $3)
              )
            ORDER BY created_at ASC, event_id ASC
            LIMIT $4
            ",
        )
        .bind(workspace)
        .bind(current_cursor.created_at)
        .bind(current_cursor.event_id)
        .bind(limit)
        .fetch_all(pool)
        .await
        .map_err(Into::into);
    }

    let mut rows: Vec<EventRow> = query_as(
        "
        SELECT event_id, workspace, actor, kind, payload, created_at
        FROM events
        WHERE workspace = $1
        ORDER BY created_at DESC, event_id DESC
        LIMIT $2
        ",
    )
    .bind(workspace)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    rows.reverse();
    Ok(rows)
}

pub(crate) async fn fetch_event_rows_after_cursor(
    pool: &PgPool,
    cursor: Option<ProjectionCursor>,
    limit: i64,
) -> ServerResult<Vec<EventRow>> {
    if let Some(current_cursor) = cursor {
        return query_as(
            "
            SELECT event_id, workspace, actor, kind, payload, created_at
            FROM events
            WHERE created_at > $1
               OR (created_at = $1 AND event_id > $2)
            ORDER BY created_at ASC, event_id ASC
            LIMIT $3
            ",
        )
        .bind(current_cursor.created_at)
        .bind(current_cursor.event_id)
        .bind(limit)
        .fetch_all(pool)
        .await
        .map_err(Into::into);
    }

    query_as(
        "
        SELECT event_id, workspace, actor, kind, payload, created_at
        FROM events
        ORDER BY created_at ASC, event_id ASC
        LIMIT $1
        ",
    )
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(Into::into)
}

pub(crate) async fn fetch_projection_cursor(
    pool: &PgPool,
    projection_name: &str,
) -> ServerResult<Option<ProjectionCursor>> {
    let row: Option<ProjectionOffsetRow> = query_as(
        "
        SELECT last_event_created_at, last_event_id
        FROM projection_offsets
        WHERE projection_name = $1
        ",
    )
    .bind(projection_name)
    .fetch_optional(pool)
    .await?;

    Ok(row.and_then(ProjectionOffsetRow::into_cursor))
}

pub(crate) async fn record_projection_cursor(
    tx: &mut Transaction<'_, Postgres>,
    projection_name: &str,
    cursor: ProjectionCursor,
    updated_at: DateTime<Utc>,
) -> ServerResult<()> {
    sqlx::query(
        "
        INSERT INTO projection_offsets (
            projection_name,
            last_event_created_at,
            last_event_id,
            updated_at
        )
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (projection_name) DO UPDATE
        SET last_event_created_at = EXCLUDED.last_event_created_at,
            last_event_id = EXCLUDED.last_event_id,
            updated_at = EXCLUDED.updated_at
        ",
    )
    .bind(projection_name)
    .bind(cursor.created_at)
    .bind(cursor.event_id)
    .bind(updated_at)
    .execute(&mut **tx)
    .await?;

    Ok(())
}

pub(crate) async fn fetch_projection_status(
    pool: &PgPool,
    projection_name: &str,
) -> ServerResult<ProjectionStatus> {
    let cursor = fetch_projection_cursor(pool, projection_name).await?;
    let total_events = count_all_events(pool).await?;
    let pending_events = count_events_after_cursor(pool, cursor).await?;

    Ok(build_projection_status(
        projection_name,
        cursor,
        total_events,
        pending_events,
    ))
}

pub(crate) async fn fetch_epic_rows_for_workspace(
    pool: &PgPool,
    workspace: &str,
) -> ServerResult<Vec<EpicRow>> {
    query_as(&format!(
        "
        {EPIC_SELECT}
        WHERE workspace = $1
        ORDER BY created_at DESC
        "
    ))
    .bind(workspace)
    .fetch_all(pool)
    .await
    .map_err(Into::into)
}

async fn count_all_events(pool: &PgPool) -> ServerResult<i64> {
    let (count,): (i64,) = query_as("SELECT COUNT(*) FROM events")
        .fetch_one(pool)
        .await?;
    Ok(count)
}

async fn count_events_after_cursor(
    pool: &PgPool,
    cursor: Option<ProjectionCursor>,
) -> ServerResult<i64> {
    if let Some(current_cursor) = cursor {
        let (count,): (i64,) = query_as(
            "
            SELECT COUNT(*)
            FROM events
            WHERE created_at > $1
               OR (created_at = $1 AND event_id > $2)
            ",
        )
        .bind(current_cursor.created_at)
        .bind(current_cursor.event_id)
        .fetch_one(pool)
        .await?;
        return Ok(count);
    }

    count_all_events(pool).await
}

pub(crate) fn build_projection_status(
    projection_name: &str,
    cursor: Option<ProjectionCursor>,
    total_events: i64,
    pending_events: i64,
) -> ProjectionStatus {
    let projected_events = total_events.saturating_sub(pending_events);
    let (last_event_created_at, last_event_id) = cursor.map_or((None, None), |current_cursor| {
        (
            Some(current_cursor.created_at.to_rfc3339()),
            Some(current_cursor.event_id),
        )
    });

    ProjectionStatus {
        caught_up: pending_events == 0,
        last_event_created_at,
        last_event_id,
        pending_events,
        projected_events,
        projection_name: projection_name.to_owned(),
        total_events,
    }
}

pub(crate) async fn fetch_epic_by_id(pool: &PgPool, epic_id: Uuid) -> ServerResult<EpicRow> {
    query_as(&format!("{EPIC_SELECT} WHERE epic_id = $1"))
        .bind(epic_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| ThreadplaneServerError::not_found("epic not found"))
}

pub(crate) async fn fetch_epic_by_event_id(
    pool: &PgPool,
    event_id: Uuid,
) -> ServerResult<Option<EpicRow>> {
    query_as(&format!("{EPIC_SELECT} WHERE event_id = $1"))
        .bind(event_id)
        .fetch_optional(pool)
        .await
        .map_err(Into::into)
}

pub(crate) async fn fetch_epic_by_id_tx(
    tx: &mut Transaction<'_, Postgres>,
    epic_id: Uuid,
    workspace: &str,
) -> ServerResult<EpicRow> {
    query_as(&format!(
        "{EPIC_SELECT} WHERE epic_id = $1 AND workspace = $2"
    ))
    .bind(epic_id)
    .bind(workspace)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| ThreadplaneServerError::not_found("epic not found"))
}

pub(crate) async fn fetch_note_by_id(pool: &PgPool, note_id: Uuid) -> ServerResult<NoteRow> {
    query_as(&format!("{NOTE_SELECT} WHERE note_id = $1"))
        .bind(note_id)
        .fetch_optional(pool)
    .await?
    .ok_or_else(|| ThreadplaneServerError::not_found("note not found"))
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct NoteListFilters<'filter> {
    pub(crate) author: Option<&'filter str>,
    pub(crate) query: Option<&'filter str>,
}

pub(crate) async fn fetch_notes_for_listing(
    pool: &PgPool,
    workspace: &str,
    filters: NoteListFilters<'_>,
    limit: i64,
) -> ServerResult<Vec<NoteRow>> {
    let normalized_author = normalize_task_owner(filters.author.map(str::to_owned));
    let normalized_query = normalized_note_query(filters.query);
    let mut query = QueryBuilder::<Postgres>::new(NOTE_SELECT);
    query.push(" WHERE workspace = ");
    query.push_bind(workspace);

    if let Some(author) = normalized_author {
        query.push(" AND author = ");
        query.push_bind(author);
    }
    if let Some(search_query) = normalized_query {
        query.push(" AND (title ILIKE ");
        query.push_bind(format!("%{search_query}%"));
        query.push(" OR body ILIKE ");
        query.push_bind(format!("%{search_query}%"));
        query.push(")");
    }

    query.push(" ORDER BY updated_at DESC, created_at DESC");
    query.push(" LIMIT ");
    query.push_bind(limit);

    query.build_query_as::<NoteRow>().fetch_all(pool).await.map_err(Into::into)
}

pub(crate) async fn fetch_note_by_event_id(
    pool: &PgPool,
    event_id: Uuid,
) -> ServerResult<Option<NoteRow>> {
    query_as(&format!("{NOTE_SELECT} WHERE event_id = $1"))
        .bind(event_id)
        .fetch_optional(pool)
        .await
        .map_err(Into::into)
}

pub(crate) async fn fetch_note_by_id_tx(
    tx: &mut Transaction<'_, Postgres>,
    note_id: Uuid,
    workspace: &str,
) -> ServerResult<NoteRow> {
    query_as(&format!(
        "{NOTE_SELECT} WHERE note_id = $1 AND workspace = $2"
    ))
    .bind(note_id)
    .bind(workspace)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| ThreadplaneServerError::not_found("note not found"))
}

pub(crate) async fn fetch_task_by_id(pool: &PgPool, task_id: Uuid) -> ServerResult<TaskRow> {
    query_as(&format!("{TASK_SELECT} WHERE task_id = $1"))
        .bind(task_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| ThreadplaneServerError::not_found("task not found"))
}

pub(crate) async fn fetch_entity_record(pool: &PgPool, entity_ref: &str) -> ServerResult<EntityRecord> {
    match parse_entity_ref(entity_ref) {
        Some(EntityRef::Epic(epic_id)) => {
            let epic = fetch_epic_by_id(pool, epic_id).await?;
            Ok(EntityRecord::Epic(EpicRecord::from(epic)))
        }
        Some(EntityRef::Note(note_id)) => {
            let note = fetch_note_by_id(pool, note_id).await?;
            Ok(EntityRecord::Note(NoteRecord::from(note)))
        }
        Some(EntityRef::Task(task_id)) => {
            let task = fetch_task_by_id(pool, task_id).await?;
            Ok(EntityRecord::Task(TaskRecord::from(task)))
        }
        None => Err(ThreadplaneServerError::bad_request(format!(
            "invalid entity ref: {entity_ref}"
        ))),
    }
}

fn normalized_note_query(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|candidate| !candidate.is_empty())
        .map(str::to_owned)
}

pub(crate) async fn fetch_task_by_event_id(
    pool: &PgPool,
    event_id: Uuid,
) -> ServerResult<Option<TaskRow>> {
    query_as(&format!("{TASK_SELECT} WHERE event_id = $1"))
        .bind(event_id)
        .fetch_optional(pool)
        .await
        .map_err(Into::into)
}

pub(crate) async fn fetch_task_by_id_tx(
    tx: &mut Transaction<'_, Postgres>,
    task_id: Uuid,
    workspace: &str,
) -> ServerResult<TaskRow> {
    query_as(&format!(
        "{TASK_SELECT} WHERE task_id = $1 AND workspace = $2"
    ))
    .bind(task_id)
    .bind(workspace)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| ThreadplaneServerError::not_found("task not found"))
}

pub(crate) async fn fetch_active_claim(
    pool: &PgPool,
    task_id: Uuid,
) -> ServerResult<Option<ClaimRow>> {
    query_as(
        "
        SELECT claim_id, task_id, workspace, actor, event_id, claimed_at, expires_at, released_at
        FROM task_claims
        WHERE task_id = $1
          AND released_at IS NULL
          AND expires_at > now()
        ORDER BY claimed_at DESC
        LIMIT 1
        ",
    )
    .bind(task_id)
    .fetch_optional(pool)
    .await
    .map_err(Into::into)
}

pub(crate) async fn fetch_claim_by_event_id(
    pool: &PgPool,
    event_id: Uuid,
) -> ServerResult<Option<ClaimRow>> {
    query_as(&format!("{CLAIM_SELECT} WHERE event_id = $1"))
        .bind(event_id)
        .fetch_optional(pool)
        .await
        .map_err(Into::into)
}

pub(crate) async fn fetch_link_by_event_id(
    pool: &PgPool,
    event_id: Uuid,
) -> ServerResult<Option<LinkRow>> {
    query_as(&format!("{LINK_SELECT} WHERE event_id = $1"))
        .bind(event_id)
        .fetch_optional(pool)
        .await
        .map_err(Into::into)
}

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

pub(crate) async fn fetch_active_claim_tx(
    tx: &mut Transaction<'_, Postgres>,
    task_id: Uuid,
) -> ServerResult<Option<ClaimRow>> {
    query_as(
        "
        SELECT claim_id, task_id, workspace, actor, event_id, claimed_at, expires_at, released_at
        FROM task_claims
        WHERE task_id = $1
          AND released_at IS NULL
          AND expires_at > now()
        ORDER BY claimed_at DESC
        LIMIT 1
        ",
    )
    .bind(task_id)
    .fetch_optional(&mut **tx)
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

pub(crate) async fn dependency_would_create_cycle(
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

pub(crate) async fn fetch_epic_for_task(
    pool: &PgPool,
    task: &TaskRow,
) -> ServerResult<Option<EpicRecord>> {
    if let Some(epic_id) = task.epic_id {
        return fetch_epic_by_id(pool, epic_id)
            .await
            .map(EpicRecord::from)
            .map(Some);
    }

    Ok(None)
}

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

    let rows = query.build_query_as::<TaskRow>().fetch_all(pool).await?;
    Ok(rows)
}

pub(crate) async fn build_task_list_entries(
    pool: &PgPool,
    tasks: Vec<TaskRow>,
) -> ServerResult<Vec<TaskListEntry>> {
    let task_ids: Vec<Uuid> = tasks.iter().map(|task| task.task_id).collect();
    let mut active_claims = fetch_active_claims_for_tasks(pool, &task_ids).await?;
    let mut dependencies = fetch_direct_dependencies_for_tasks(pool, &task_ids).await?;
    let mut dependents = fetch_direct_dependents_for_tasks(pool, &task_ids).await?;
    let mut epics = fetch_epics_for_tasks(pool, &tasks).await?;
    let ready_states = fetch_ready_states_for_tasks(pool, &task_ids).await?;
    let mut entries = Vec::with_capacity(tasks.len());
    for task in tasks {
        let task_id = task.task_id;
        let epic_id = task.epic_id;
        entries.push(TaskListEntry {
            active_claim: active_claims.remove(&task_id),
            dependencies: dependencies.remove(&task_id).unwrap_or_default(),
            dependents: dependents.remove(&task_id).unwrap_or_default(),
            epic: epic_id.and_then(|value| epics.remove(&value)),
            ready: ready_states.get(&task_id).copied().unwrap_or(false),
            task: task.into(),
        });
    }
    Ok(entries)
}

async fn fetch_active_claims_for_tasks(
    pool: &PgPool,
    task_ids: &[Uuid],
) -> ServerResult<BTreeMap<Uuid, TaskClaimRecord>> {
    if task_ids.is_empty() {
        return Ok(BTreeMap::new());
    }

    let rows: Vec<ClaimRow> = query_as(
        "
        SELECT DISTINCT ON (task_id)
            claim_id,
            task_id,
            workspace,
            actor,
            event_id,
            claimed_at,
            expires_at,
            released_at
        FROM task_claims
        WHERE task_id = ANY($1)
          AND released_at IS NULL
          AND expires_at > now()
        ORDER BY task_id, claimed_at DESC
        ",
    )
    .bind(task_ids)
    .fetch_all(pool)
    .await?;

    let mut claims = BTreeMap::new();
    for row in rows {
        claims.insert(row.task_id, row.into());
    }
    Ok(claims)
}

async fn fetch_dependency_rows_for_tasks(
    pool: &PgPool,
    task_ids: &[Uuid],
    reverse: bool,
) -> ServerResult<BTreeMap<Uuid, Vec<TaskDependencySummary>>> {
    if task_ids.is_empty() {
        return Ok(BTreeMap::new());
    }

    let sql = if reverse {
        "
        SELECT
            td.depends_on_task_id AS source_task_id,
            t.task_id AS dependency_id,
            t.title,
            t.status
        FROM task_dependencies td
        JOIN tasks t ON t.task_id = td.task_id
        WHERE td.depends_on_task_id = ANY($1)
        ORDER BY td.depends_on_task_id, t.created_at DESC
        "
    } else {
        "
        SELECT
            td.task_id AS source_task_id,
            t.task_id AS dependency_id,
            t.title,
            t.status
        FROM task_dependencies td
        JOIN tasks t ON t.task_id = td.depends_on_task_id
        WHERE td.task_id = ANY($1)
        ORDER BY td.task_id, t.created_at DESC
        "
    };

    let rows: Vec<TaskDependencyListRow> = query_as(sql).bind(task_ids).fetch_all(pool).await?;
    let mut dependencies = BTreeMap::new();
    for row in rows {
        dependencies
            .entry(row.source_task_id)
            .or_insert_with(Vec::new)
            .push(TaskDependencySummary {
                depth: 1,
                entity_ref: task_entity_ref(row.dependency_id),
                status: row.status,
                task_id: row.dependency_id,
                title: row.title,
            });
    }

    Ok(dependencies)
}

async fn fetch_direct_dependencies_for_tasks(
    pool: &PgPool,
    task_ids: &[Uuid],
) -> ServerResult<BTreeMap<Uuid, Vec<TaskDependencySummary>>> {
    fetch_dependency_rows_for_tasks(pool, task_ids, false).await
}

async fn fetch_direct_dependents_for_tasks(
    pool: &PgPool,
    task_ids: &[Uuid],
) -> ServerResult<BTreeMap<Uuid, Vec<TaskDependencySummary>>> {
    fetch_dependency_rows_for_tasks(pool, task_ids, true).await
}

async fn fetch_epics_for_tasks(
    pool: &PgPool,
    tasks: &[TaskRow],
) -> ServerResult<BTreeMap<Uuid, EpicRecord>> {
    let mut epic_ids = Vec::new();
    for task in tasks {
        if let Some(epic_id) = task.epic_id {
            if !epic_ids.contains(&epic_id) {
                epic_ids.push(epic_id);
            }
        }
    }
    if epic_ids.is_empty() {
        return Ok(BTreeMap::new());
    }

    let rows: Vec<EpicRow> = query_as(&format!("{EPIC_SELECT} WHERE epic_id = ANY($1)"))
        .bind(epic_ids)
        .fetch_all(pool)
        .await?;

    let mut epics = BTreeMap::new();
    for row in rows {
        epics.insert(row.epic_id, row.into());
    }
    Ok(epics)
}

async fn fetch_ready_states_for_tasks(
    pool: &PgPool,
    task_ids: &[Uuid],
) -> ServerResult<BTreeMap<Uuid, bool>> {
    if task_ids.is_empty() {
        return Ok(BTreeMap::new());
    }

    let rows: Vec<TaskReadyRow> = query_as(
        "
        SELECT
            t.task_id,
            CASE
                WHEN t.status <> 'open' THEN false
                WHEN EXISTS (
                    SELECT 1
                    FROM task_dependencies td
                    JOIN tasks dependency ON dependency.task_id = td.depends_on_task_id
                    WHERE td.task_id = t.task_id
                      AND dependency.status <> 'completed'
                ) THEN false
                ELSE true
            END AS ready
        FROM tasks t
        WHERE t.task_id = ANY($1)
        ",
    )
    .bind(task_ids)
    .fetch_all(pool)
    .await?;

    let mut ready_states = BTreeMap::new();
    for row in rows {
        ready_states.insert(row.task_id, row.ready);
    }
    Ok(ready_states)
}

pub(crate) async fn fetch_direct_dependencies(
    pool: &PgPool,
    task_id: Uuid,
) -> ServerResult<Vec<TaskDependencySummary>> {
    fetch_dependency_rows(pool, task_id, false).await
}

pub(crate) async fn fetch_direct_dependents(
    pool: &PgPool,
    task_id: Uuid,
) -> ServerResult<Vec<TaskDependencySummary>> {
    fetch_dependency_rows(pool, task_id, true).await
}

pub(crate) async fn fetch_dependency_chain(
    pool: &PgPool,
    task_id: Uuid,
) -> ServerResult<Vec<TaskDependencySummary>> {
    fetch_dependency_chain_rows(pool, task_id, false).await
}

pub(crate) async fn fetch_dependent_chain(
    pool: &PgPool,
    task_id: Uuid,
) -> ServerResult<Vec<TaskDependencySummary>> {
    fetch_dependency_chain_rows(pool, task_id, true).await
}

async fn fetch_dependency_rows(
    pool: &PgPool,
    task_id: Uuid,
    reverse: bool,
) -> ServerResult<Vec<TaskDependencySummary>> {
    let sql = if reverse {
        "
        SELECT t.task_id, t.title, t.status
        FROM task_dependencies td
        JOIN tasks t ON t.task_id = td.task_id
        WHERE td.depends_on_task_id = $1
        ORDER BY t.created_at DESC
        "
    } else {
        "
        SELECT t.task_id, t.title, t.status
        FROM task_dependencies td
        JOIN tasks t ON t.task_id = td.depends_on_task_id
        WHERE td.task_id = $1
        ORDER BY t.created_at DESC
        "
    };

    let rows: Vec<(Uuid, String, String)> = query_as(sql).bind(task_id).fetch_all(pool).await?;

    Ok(rows
        .into_iter()
        .map(|(dependency_id, title, status)| TaskDependencySummary {
            depth: 1,
            entity_ref: task_entity_ref(dependency_id),
            status,
            task_id: dependency_id,
            title,
        })
        .collect())
}

async fn fetch_dependency_chain_rows(
    pool: &PgPool,
    task_id: Uuid,
    reverse: bool,
) -> ServerResult<Vec<TaskDependencySummary>> {
    let sql = if reverse {
        "
        WITH RECURSIVE dependency_chain(task_id, depth) AS (
            SELECT td.task_id, 1
            FROM task_dependencies td
            WHERE td.depends_on_task_id = $1
            UNION
            SELECT td.task_id, dependency_chain.depth + 1
            FROM task_dependencies td
            JOIN dependency_chain ON td.depends_on_task_id = dependency_chain.task_id
        )
        SELECT t.task_id, t.title, t.status, MIN(dependency_chain.depth) AS depth
        FROM dependency_chain
        JOIN tasks t ON t.task_id = dependency_chain.task_id
        GROUP BY t.task_id, t.title, t.status
        ORDER BY depth, t.created_at DESC
        "
    } else {
        "
        WITH RECURSIVE dependency_chain(task_id, depth) AS (
            SELECT td.depends_on_task_id, 1
            FROM task_dependencies td
            WHERE td.task_id = $1
            UNION
            SELECT td.depends_on_task_id, dependency_chain.depth + 1
            FROM task_dependencies td
            JOIN dependency_chain ON td.task_id = dependency_chain.task_id
        )
        SELECT t.task_id, t.title, t.status, MIN(dependency_chain.depth) AS depth
        FROM dependency_chain
        JOIN tasks t ON t.task_id = dependency_chain.task_id
        GROUP BY t.task_id, t.title, t.status
        ORDER BY depth, t.created_at DESC
        "
    };

    let rows: Vec<(Uuid, String, String, i32)> =
        query_as(sql).bind(task_id).fetch_all(pool).await?;

    Ok(rows
        .into_iter()
        .map(
            |(dependency_id, title, status, depth)| TaskDependencySummary {
                depth,
                entity_ref: task_entity_ref(dependency_id),
                status,
                task_id: dependency_id,
                title,
            },
        )
        .collect())
}

pub(crate) async fn task_is_ready(pool: &PgPool, task_id: Uuid) -> ServerResult<bool> {
    let task = fetch_task_by_id(pool, task_id).await?;
    if task.status != "open" {
        return Ok(false);
    }

    let unresolved: (i64,) = query_as(
        "
        SELECT COUNT(*)
        FROM task_dependencies td
        JOIN tasks dependency ON dependency.task_id = td.depends_on_task_id
        WHERE td.task_id = $1
          AND dependency.status <> 'completed'
        ",
    )
    .bind(task_id)
    .fetch_one(pool)
    .await?;

    Ok(unresolved.0 == 0)
}

pub(crate) async fn fetch_text_entity_by_ref_tx(
    tx: &mut Transaction<'_, Postgres>,
    workspace: &str,
    entity_ref: &str,
) -> ServerResult<TextEntityRow> {
    match parse_entity_ref(entity_ref) {
        Some(EntityRef::Epic(_)) => Err(ThreadplaneServerError::bad_request(format!(
            "epic refs are not textual entities: {entity_ref}"
        ))),
        Some(EntityRef::Note(note_id)) => Ok(TextEntityRow::Note(
            fetch_note_by_id_tx(tx, note_id, workspace).await?,
        )),
        Some(EntityRef::Task(task_id)) => Ok(TextEntityRow::Task(
            fetch_task_by_id_tx(tx, task_id, workspace).await?,
        )),
        None => Err(ThreadplaneServerError::bad_request(format!(
            "unsupported entity ref {entity_ref}"
        ))),
    }
}

pub(crate) async fn group_exists(
    tx: &mut Transaction<'_, Postgres>,
    transclusion_id: Uuid,
) -> ServerResult<bool> {
    let exists: Option<(Uuid,)> =
        query_as("SELECT transclusion_id FROM transclusion_groups WHERE transclusion_id = $1")
            .bind(transclusion_id)
            .fetch_optional(&mut **tx)
            .await?;
    Ok(exists.is_some())
}

pub(crate) async fn insert_transclusion_group(
    tx: &mut Transaction<'_, Postgres>,
    transclusion_id: Uuid,
    workspace: &str,
    actor: &str,
    title: &str,
    content: &str,
    now: DateTime<Utc>,
) -> ServerResult<()> {
    sqlx::query(
        "
        INSERT INTO transclusion_groups (
            transclusion_id,
            workspace,
            created_by,
            title,
            content,
            created_at,
            updated_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $6)
        ",
    )
    .bind(transclusion_id)
    .bind(workspace)
    .bind(actor)
    .bind(title)
    .bind(content)
    .bind(now)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub(crate) async fn update_transclusion_group(
    tx: &mut Transaction<'_, Postgres>,
    transclusion_id: Uuid,
    workspace: &str,
    _actor: &str,
    title: &str,
    content: &str,
    now: DateTime<Utc>,
) -> ServerResult<()> {
    sqlx::query(
        "
        UPDATE transclusion_groups
        SET workspace = $2,
            title = $3,
            content = $4,
            updated_at = $5
        WHERE transclusion_id = $1
        ",
    )
    .bind(transclusion_id)
    .bind(workspace)
    .bind(title)
    .bind(content)
    .bind(now)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub(crate) async fn move_group_members(
    tx: &mut Transaction<'_, Postgres>,
    from_group_id: Uuid,
    to_group_id: Uuid,
) -> ServerResult<()> {
    sqlx::query("UPDATE notes SET transclusion_id = $2 WHERE transclusion_id = $1")
        .bind(from_group_id)
        .bind(to_group_id)
        .execute(&mut **tx)
        .await?;
    sqlx::query("UPDATE tasks SET transclusion_id = $2 WHERE transclusion_id = $1")
        .bind(from_group_id)
        .bind(to_group_id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

pub(crate) async fn set_entity_transclusion(
    tx: &mut Transaction<'_, Postgres>,
    entity: &TextEntityRow,
    transclusion_id: Uuid,
) -> ServerResult<()> {
    match entity {
        TextEntityRow::Note(note) => {
            sqlx::query("UPDATE notes SET transclusion_id = $2 WHERE note_id = $1")
                .bind(note.note_id)
                .bind(transclusion_id)
                .execute(&mut **tx)
                .await?;
        }
        TextEntityRow::Task(task) => {
            sqlx::query("UPDATE tasks SET transclusion_id = $2 WHERE task_id = $1")
                .bind(task.task_id)
                .bind(transclusion_id)
                .execute(&mut **tx)
                .await?;
        }
    }
    Ok(())
}

pub(crate) async fn sync_transclusion_members(
    tx: &mut Transaction<'_, Postgres>,
    transclusion_id: Uuid,
) -> ServerResult<()> {
    let group: TransclusionGroupRow = query_as(
        "
        SELECT transclusion_id, workspace, title, content, created_at, updated_at
        FROM transclusion_groups
        WHERE transclusion_id = $1
        ",
    )
    .bind(transclusion_id)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| ThreadplaneServerError::not_found("transclusion group not found"))?;

    sqlx::query(
        "
        UPDATE notes
        SET title = $2,
            body = $3,
            updated_at = $4
        WHERE transclusion_id = $1
        ",
    )
    .bind(transclusion_id)
    .bind(&group.title)
    .bind(&group.content)
    .bind(group.updated_at)
    .execute(&mut **tx)
    .await?;

    sqlx::query(
        "
        UPDATE tasks
        SET title = $2,
            details = $3,
            updated_at = $4
        WHERE transclusion_id = $1
        ",
    )
    .bind(transclusion_id)
    .bind(&group.title)
    .bind(&group.content)
    .bind(group.updated_at)
    .execute(&mut **tx)
    .await?;

    Ok(())
}

pub(crate) fn event_kind_name(kind: EventKind) -> String {
    kind.to_string()
}

pub(crate) fn parse_event_kind(value: &str) -> EventKind {
    EventKind::from_str(value).unwrap_or(EventKind::NoteRecorded)
}

#[derive(Debug, FromRow, Clone)]
pub(crate) struct EventRow {
    pub(crate) event_id: Uuid,
    pub(crate) workspace: String,
    pub(crate) actor: String,
    pub(crate) kind: String,
    pub(crate) payload: Value,
    pub(crate) created_at: DateTime<Utc>,
}

impl EventRow {
    #[must_use]
    pub(crate) const fn cursor(&self) -> ProjectionCursor {
        ProjectionCursor::new(self.created_at, self.event_id)
    }

    #[must_use]
    pub(crate) fn parsed_kind(&self) -> EventKind {
        parse_event_kind(&self.kind)
    }
}

#[derive(Debug, FromRow, Clone)]
pub(crate) struct EpicRow {
    epic_id: Uuid,
    event_id: Uuid,
    workspace: String,
    author: String,
    title: String,
    body: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, FromRow, Clone)]
pub(crate) struct NoteRow {
    pub(crate) note_id: Uuid,
    event_id: Uuid,
    workspace: String,
    author: String,
    title: String,
    body: String,
    pub(crate) transclusion_id: Option<Uuid>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, FromRow, Clone)]
pub(crate) struct TaskRow {
    pub(crate) task_id: Uuid,
    event_id: Uuid,
    workspace: String,
    author: String,
    title: String,
    details: String,
    pub(crate) status: String,
    pub(crate) epic_id: Option<Uuid>,
    priority: String,
    owner: Option<String>,
    labels: Vec<String>,
    pub(crate) transclusion_id: Option<Uuid>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, FromRow)]
pub(crate) struct ClaimRow {
    pub(crate) claim_id: Uuid,
    task_id: Uuid,
    workspace: String,
    pub(crate) actor: String,
    event_id: Uuid,
    claimed_at: DateTime<Utc>,
    pub(crate) expires_at: DateTime<Utc>,
    #[sqlx(rename = "released_at")]
    _released_at: Option<DateTime<Utc>>,
}

#[derive(Debug, FromRow)]
struct ProjectionOffsetRow {
    last_event_created_at: Option<DateTime<Utc>>,
    last_event_id: Option<Uuid>,
}

impl ProjectionOffsetRow {
    const fn into_cursor(self) -> Option<ProjectionCursor> {
        match (self.last_event_created_at, self.last_event_id) {
            (Some(created_at), Some(event_id)) => Some(ProjectionCursor::new(created_at, event_id)),
            _ => None,
        }
    }
}

#[derive(Debug, FromRow)]
pub(crate) struct LinkRow {
    pub(crate) link_id: Uuid,
    event_id: Uuid,
    workspace: String,
    actor: String,
    from_entity_ref: String,
    to_entity_ref: String,
    relation: String,
    is_xanadu: bool,
    transclusion_id: Option<Uuid>,
    created_at: DateTime<Utc>,
}

#[derive(Debug, FromRow)]
pub(crate) struct TaskDependencyRow {
    pub(crate) task_id: Uuid,
    pub(crate) depends_on_task_id: Uuid,
}

#[derive(Debug, FromRow)]
struct TaskDependencyListRow {
    dependency_id: Uuid,
    source_task_id: Uuid,
    status: String,
    title: String,
}

#[derive(Debug, FromRow)]
struct TaskReadyRow {
    ready: bool,
    task_id: Uuid,
}

#[derive(Debug, FromRow)]
struct TransclusionGroupRow {
    title: String,
    content: String,
    updated_at: DateTime<Utc>,
}

pub(crate) enum TextEntityRow {
    Note(NoteRow),
    Task(TaskRow),
}

impl TextEntityRow {
    pub(crate) const fn transclusion_id(&self) -> Option<Uuid> {
        match self {
            Self::Note(note) => note.transclusion_id,
            Self::Task(task) => task.transclusion_id,
        }
    }

    pub(crate) fn title(&self) -> &str {
        match self {
            Self::Note(note) => &note.title,
            Self::Task(task) => &task.title,
        }
    }

    pub(crate) fn content(&self) -> &str {
        match self {
            Self::Note(note) => &note.body,
            Self::Task(task) => &task.details,
        }
    }
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

impl From<EventRow> for EventRecord {
    #[inline]
    fn from(value: EventRow) -> Self {
        Self {
            event_id: value.event_id,
            workspace: value.workspace,
            actor: value.actor,
            kind: parse_event_kind(&value.kind),
            payload: value.payload,
            created_at: value.created_at.to_rfc3339(),
        }
    }
}

impl From<EpicRow> for EpicRecord {
    #[inline]
    fn from(value: EpicRow) -> Self {
        Self {
            epic_id: value.epic_id,
            entity_ref: epic_entity_ref(value.epic_id),
            event_id: value.event_id,
            workspace: value.workspace,
            author: value.author,
            title: value.title,
            body: value.body,
            created_at: value.created_at.to_rfc3339(),
            updated_at: value.updated_at.to_rfc3339(),
        }
    }
}

impl From<NoteRow> for threadplane_core::NoteRecord {
    #[inline]
    fn from(value: NoteRow) -> Self {
        Self {
            note_id: value.note_id,
            entity_ref: note_entity_ref(value.note_id),
            event_id: value.event_id,
            workspace: value.workspace,
            author: value.author,
            title: value.title,
            body: value.body,
            transclusion_id: value.transclusion_id,
            created_at: value.created_at.to_rfc3339(),
            updated_at: value.updated_at.to_rfc3339(),
        }
    }
}

impl From<TaskRow> for TaskRecord {
    #[inline]
    fn from(value: TaskRow) -> Self {
        let metadata = task_metadata_from_row(&value);
        Self {
            task_id: value.task_id,
            entity_ref: task_entity_ref(value.task_id),
            event_id: value.event_id,
            workspace: value.workspace,
            author: value.author,
            title: value.title,
            details: value.details,
            status: value.status,
            epic_id: value.epic_id,
            metadata,
            transclusion_id: value.transclusion_id,
            created_at: value.created_at.to_rfc3339(),
            updated_at: value.updated_at.to_rfc3339(),
        }
    }
}

impl From<TaskRow> for TaskSummary {
    #[inline]
    fn from(value: TaskRow) -> Self {
        let metadata = task_metadata_from_row(&value);
        Self {
            task_id: value.task_id,
            entity_ref: task_entity_ref(value.task_id),
            workspace: value.workspace,
            title: value.title,
            details: value.details,
            status: value.status,
            epic_id: value.epic_id,
            author: value.author,
            metadata,
            transclusion_id: value.transclusion_id,
            created_at: value.created_at.to_rfc3339(),
            updated_at: value.updated_at.to_rfc3339(),
        }
    }
}

fn task_metadata_from_row(value: &TaskRow) -> TaskMetadata {
    TaskMetadata {
        labels: normalize_task_labels(value.labels.clone()),
        owner: normalize_task_owner(value.owner.clone()),
        priority: parse_task_priority(&value.priority),
    }
}

fn parse_task_priority(value: &str) -> TaskPriority {
    TaskPriority::from_lossy(value)
}

impl From<ClaimRow> for TaskClaimRecord {
    #[inline]
    fn from(value: ClaimRow) -> Self {
        Self {
            claim_id: value.claim_id,
            task_id: value.task_id,
            workspace: value.workspace,
            actor: value.actor,
            event_id: value.event_id,
            claimed_at: value.claimed_at.to_rfc3339(),
            expires_at: value.expires_at.to_rfc3339(),
        }
    }
}

impl From<LinkRow> for threadplane_core::LinkRecord {
    #[inline]
    fn from(value: LinkRow) -> Self {
        Self {
            link_id: value.link_id,
            event_id: value.event_id,
            workspace: value.workspace,
            actor: value.actor,
            from: value.from_entity_ref,
            to: value.to_entity_ref,
            relation: value.relation,
            is_xanadu: value.is_xanadu,
            transclusion_id: value.transclusion_id,
            created_at: value.created_at.to_rfc3339(),
        }
    }
}

pub(crate) struct XanaduGroup {
    pub(crate) canonical_group_id: Uuid,
    pub(crate) merged_group_id: Option<Uuid>,
}

pub(crate) async fn prepare_xanadu_group(
    tx: &mut Transaction<'_, Postgres>,
    request: &threadplane_core::CreateXanaduLinkRequest,
    created_at: DateTime<Utc>,
) -> ServerResult<XanaduGroup> {
    let from = fetch_text_entity_by_ref_tx(tx, &request.workspace, &request.from).await?;
    let to = fetch_text_entity_by_ref_tx(tx, &request.workspace, &request.to).await?;
    let canonical_group_id = from
        .transclusion_id()
        .or_else(|| to.transclusion_id())
        .unwrap_or_else(Uuid::new_v4);
    let merged_group_id = match (from.transclusion_id(), to.transclusion_id()) {
        (Some(left), Some(right)) if left != right => Some(right),
        _ => None,
    };

    upsert_xanadu_group(
        tx,
        request,
        &from,
        canonical_group_id,
        merged_group_id,
        created_at,
    )
    .await?;
    set_entity_transclusion(tx, &from, canonical_group_id).await?;
    set_entity_transclusion(tx, &to, canonical_group_id).await?;
    sync_transclusion_members(tx, canonical_group_id).await?;

    Ok(XanaduGroup {
        canonical_group_id,
        merged_group_id,
    })
}

async fn upsert_xanadu_group(
    tx: &mut Transaction<'_, Postgres>,
    request: &threadplane_core::CreateXanaduLinkRequest,
    from: &TextEntityRow,
    canonical_group_id: Uuid,
    merged_group_id: Option<Uuid>,
    created_at: DateTime<Utc>,
) -> ServerResult<()> {
    let source_title = from.title().to_owned();
    let source_content = from.content().to_owned();

    if group_exists(tx, canonical_group_id).await? {
        update_transclusion_group(
            tx,
            canonical_group_id,
            &request.workspace,
            &request.actor,
            &source_title,
            &source_content,
            created_at,
        )
        .await?;
    } else {
        insert_transclusion_group(
            tx,
            canonical_group_id,
            &request.workspace,
            &request.actor,
            &source_title,
            &source_content,
            created_at,
        )
        .await?;
    }

    if let Some(group_id) = merged_group_id {
        move_group_members(tx, group_id, canonical_group_id).await?;
        sqlx::query("DELETE FROM transclusion_groups WHERE transclusion_id = $1")
            .bind(group_id)
            .execute(&mut **tx)
            .await?;
    }

    Ok(())
}
