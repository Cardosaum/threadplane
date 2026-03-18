#![expect(
    clippy::redundant_pub_crate,
    reason = "Handlers are crate-local endpoints with explicit visibility."
)]

mod content;
mod links;
mod reads;
mod tasks;
mod workspace;

use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    Json,
};
use bon::builder;
use core::future::Future;
use serde::Deserialize;
use tracing::error;

use crate::{
    build_info::current_build_info,
    idempotency::{
        begin_idempotent_command, complete_idempotent_command, CommandExecution,
        IdempotencyContext, IDEMPOTENCY_KEY_HEADER,
    },
    lifecycle::{calculate_claim_expiry, normalized_lease_seconds},
    prelude::*,
    projections::{
        fetch_entity_relations, project_claim, project_epic, project_link, project_memory,
        project_note, project_task, project_task_dependency_by_id,
        project_task_supporting_entities, reproject_transclusion_group,
    },
    replay::GRAPH_PROJECTION_NAME,
    storage::{
        append_event, append_task_dependency, build_task_list_entries, ensure_workspace_governance,
        fetch_active_claim, fetch_active_claim_tx, fetch_actor_public_keys, fetch_dependency_chain,
        fetch_dependent_chain, fetch_direct_dependencies, fetch_direct_dependents,
        fetch_entity_record, fetch_epic_by_id, fetch_epic_by_id_tx, fetch_epic_for_task,
        fetch_epic_rows_for_workspace, fetch_event_row_for_workspace,
        fetch_event_rows_after_workspace_cursor, fetch_event_rows_for_workspace,
        fetch_memories_for_listing, fetch_memory_by_id, fetch_memory_by_id_tx, fetch_note_by_id,
        fetch_note_by_id_tx, fetch_notes_for_listing, fetch_projection_status, fetch_task_by_id,
        fetch_task_by_id_tx, fetch_tasks_for_listing, fetch_workspace_memberships,
        prepare_xanadu_group, record_projection_cursor, require_workspace_role,
        sync_transclusion_members, task_is_ready, unique_task_ids, update_transclusion_group,
        upsert_actor_public_key, upsert_workspace_membership, upsert_workspace_policy,
        workspace_supports_priority, MemoryListFilters, NoteListFilters, TaskListFilters, TaskRow,
    },
};

use threadplane_core::{
    health_summary, normalize_memory_recall_triggers, normalize_memory_tags, normalize_task_labels,
    normalize_task_owner, scope_summary, service_snapshot, ActorPublicKey, AddLinkRequest,
    AddTaskDependencyRequest, AddWorkspacePublicKeyRequest, ApiEnvelope, ClaimNextTaskRequest,
    ClaimTaskRequest, CompleteTaskRequest, CreateEpicRequest, CreateMemoryRequest,
    CreateNoteRequest, CreateXanaduLinkRequest, EntityContext, EpicRecord, EventKind, EventRecord,
    GrantWorkspaceMembershipRequest, LinkRecord, MemoryAudience, MemoryImportance, MemoryKind,
    MemoryRecord, NoteRecord, OfferTaskRequest, ProjectionStatus, ReleaseTaskRequest,
    ServiceSnapshot, TaskClaimRecord, TaskContext, TaskDag, TaskListEntry, TaskPriority,
    TaskRecord, UpdateNoteRequest, UpdateTaskRequest, UpdateWorkspacePolicyRequest,
    WorkspaceMembership, WorkspacePolicy, WorkspaceRole, DEPENDS_ON_RELATION, XANADU_RELATION,
};

pub(crate) use content::{create_epic, create_memory, create_note, update_note};
pub(crate) use links::{add_link, add_xanadu_link};
pub(crate) use reads::{
    healthz, list_epics, list_events, list_memories, list_notes, list_open_tasks, list_tasks,
    list_workspace_memberships, list_workspace_public_keys, next_task, prime_memories,
    projection_status, related_entities, root, scope, show_entity, show_epic, show_memory,
    show_note, show_task, show_workspace_policy, tail_events, task_context, task_dag,
};
pub(crate) use tasks::{
    add_task_dependency, claim_next_task, claim_task, complete_task, offer_task, release_task,
    task_next_filters, task_selection_filters, update_task,
};
pub(crate) use workspace::{
    add_workspace_public_key, grant_workspace_membership, update_workspace_policy,
};

const DEFAULT_LIST_LIMIT: i64 = 25;
const MAX_LIST_LIMIT: i64 = 200;

#[derive(Debug, Deserialize)]
pub(crate) struct EntityPath {
    entity_ref: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct EpicPath {
    epic_id: Uuid,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ListQuery {
    limit: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct MemoryPath {
    memory_id: Uuid,
}

#[derive(Debug, Deserialize)]
pub(crate) struct EventTailQuery {
    after_event_id: Option<Uuid>,
    limit: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct NoteListQuery {
    author: Option<String>,
    limit: Option<i64>,
    query: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct NotePath {
    note_id: Uuid,
}

#[derive(Debug, Deserialize)]
pub(crate) struct MemoryListQuery {
    audience: Option<MemoryAudience>,
    importance: Option<MemoryImportance>,
    kind: Option<MemoryKind>,
    limit: Option<i64>,
    query: Option<String>,
    recall_trigger: Option<String>,
    tag: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct TaskListQuery {
    epic_id: Option<Uuid>,
    label: Option<String>,
    limit: Option<i64>,
    owner: Option<String>,
    priority: Option<TaskPriority>,
    ready_only: Option<bool>,
    status: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct TaskPath {
    task_id: Uuid,
}

#[derive(Debug, Deserialize)]
pub(crate) struct WorkspaceKeysQuery {
    actor_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct WorkspacePath {
    workspace: String,
}

fn with_projection_status(
    mut payload: Value,
    projection: ProjectionStatus,
) -> Result<Value, serde_json::Error> {
    if let Value::Object(ref mut object) = payload {
        object.insert("projection".to_owned(), serde_json::to_value(projection)?);
    }

    Ok(payload)
}

const fn success<T>(data: T) -> Json<ApiEnvelope<T>> {
    Json(ApiEnvelope {
        data,
        ok: true,
        receipt: None,
    })
}

const fn success_with_receipt<T>(
    data: T,
    receipt: Option<threadplane_core::CommandReceipt>,
) -> Json<ApiEnvelope<T>> {
    Json(ApiEnvelope {
        data,
        ok: true,
        receipt,
    })
}

fn idempotency_key(headers: &HeaderMap) -> ServerResult<Option<&str>> {
    headers
        .get(IDEMPOTENCY_KEY_HEADER)
        .map(|value| {
            value.to_str().map_err(|_invalid_header| {
                ThreadplaneServerError::bad_request(
                    "idempotency key must be a valid ASCII-compatible header value",
                )
            })
        })
        .transpose()
}

async fn ensure_workspace_policy(
    pool: &PgPool,
    bootstrap: &WorkspaceGovernanceBootstrap,
    workspace: &str,
) -> ServerResult<WorkspacePolicy> {
    ensure_workspace_governance(pool, workspace, bootstrap).await
}

#[builder]
async fn require_workspace_editor(
    actor: &str,
    bootstrap: &WorkspaceGovernanceBootstrap,
    pool: &PgPool,
    workspace: &str,
) -> ServerResult<WorkspaceRole> {
    ensure_workspace_policy(pool, bootstrap, workspace).await?;
    require_workspace_role(
        pool,
        workspace,
        actor,
        WorkspaceRole::can_edit,
        "edit workspace state",
    )
    .await
}

#[builder]
async fn require_workspace_admin(
    actor: &str,
    bootstrap: &WorkspaceGovernanceBootstrap,
    pool: &PgPool,
    workspace: &str,
) -> ServerResult<WorkspaceRole> {
    ensure_workspace_policy(pool, bootstrap, workspace).await?;
    require_workspace_role(
        pool,
        workspace,
        actor,
        WorkspaceRole::can_administer,
        "administer workspace policy",
    )
    .await
}

async fn advance_graph_projection_cursor(
    pool: &PgPool,
    workspace: &str,
    event_id: Uuid,
) -> ServerResult<()> {
    let event = fetch_event_row_for_workspace(pool, workspace, event_id).await?;
    let mut tx = pool.begin().await?;
    record_projection_cursor(&mut tx, GRAPH_PROJECTION_NAME, event.cursor(), Utc::now()).await?;
    tx.commit().await?;
    Ok(())
}

#[builder]
async fn project_graph_event<Output, Operation>(
    event_id: Uuid,
    operation: Operation,
    pool: &PgPool,
    projection_coordinator: &ProjectionCoordinator,
    workspace: &str,
) -> ServerResult<Output>
where
    Operation: Future<Output = ServerResult<Output>>,
{
    let output = projection_coordinator.run(operation).await?;
    advance_graph_projection_cursor(pool, workspace, event_id).await?;
    Ok(output)
}

#[inline]
#[must_use]
pub(crate) fn normalized_list_limit(limit: Option<i64>) -> i64 {
    limit.unwrap_or(DEFAULT_LIST_LIMIT).clamp(1, MAX_LIST_LIMIT)
}
