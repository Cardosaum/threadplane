#![expect(
    clippy::redundant_pub_crate,
    reason = "Handlers are crate-local endpoints with explicit visibility."
)]

mod reads;
mod tasks;

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

async fn project_memory_record(graph: &Graph, record: &MemoryRecord) -> ServerResult<()> {
    project_memory(graph, record).await.map_err(|error| {
        error!(?error, memory_id = %record.memory_id, "failed to project memory");
        ThreadplaneServerError::internal(error)
    })
}

fn build_xanadu_request_payload(request: &CreateXanaduLinkRequest) -> Value {
    json!({
        "workspace": request.workspace,
        "actor": request.actor,
        "from": request.from,
        "to": request.to,
        "transclusion_id": null,
        "merged_group_id": null,
    })
}

fn build_xanadu_event_payload(
    request: &CreateXanaduLinkRequest,
    canonical_group_id: Uuid,
    merged_group_id: Option<Uuid>,
) -> Value {
    json!({
        "workspace": request.workspace,
        "actor": request.actor,
        "from": request.from,
        "to": request.to,
        "transclusion_id": canonical_group_id,
        "merged_group_id": merged_group_id,
    })
}

#[builder]
async fn persist_xanadu_link(
    canonical_group_id: Uuid,
    created_at: DateTime<Utc>,
    event_id: Uuid,
    request: &CreateXanaduLinkRequest,
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
) -> ServerResult<LinkRecord> {
    let link_id = Uuid::new_v4();
    sqlx::query(
        "
        INSERT INTO links (
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
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, true, $8, $9)
        ",
    )
    .bind(link_id)
    .bind(event_id)
    .bind(&request.workspace)
    .bind(&request.actor)
    .bind(&request.from)
    .bind(&request.to)
    .bind(XANADU_RELATION)
    .bind(canonical_group_id)
    .bind(created_at)
    .execute(&mut **tx)
    .await?;

    Ok(LinkRecord {
        link_id,
        event_id,
        workspace: request.workspace.clone(),
        actor: request.actor.clone(),
        from: request.from.clone(),
        to: request.to.clone(),
        relation: XANADU_RELATION.to_owned(),
        is_xanadu: true,
        transclusion_id: Some(canonical_group_id),
        created_at: created_at.to_rfc3339(),
    })
}

pub(crate) async fn create_epic(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateEpicRequest>,
) -> AppResult<EpicRecord> {
    require_workspace_editor()
        .pool(state.pool())
        .bootstrap(state.bootstrap())
        .workspace(&request.workspace)
        .actor(&request.author)
        .call()
        .await?;
    let mut tx = state.pool().begin().await?;
    let epic_id = Uuid::new_v4();
    let created_at = Utc::now();
    let payload = serde_json::to_value(&request)?;
    let pending_receipt = match begin_idempotent_command::<EpicRecord>(
        &mut tx,
        IdempotencyContext {
            actor: &request.author,
            command_kind: "create_epic",
            idempotency_key: idempotency_key(&headers)?,
            request_payload: &payload,
            workspace: &request.workspace,
        },
        created_at,
    )
    .await?
    {
        CommandExecution::Execute(pending_receipt) => pending_receipt,
        CommandExecution::Replay(envelope) => {
            tx.commit().await?;
            return Ok(Json(envelope));
        }
    };
    let event_id = append_event(
        &mut tx,
        &request.workspace,
        &request.author,
        EventKind::EpicRecorded,
        &payload,
        created_at,
    )
    .await?;

    sqlx::query(
        "
        INSERT INTO epics (
            epic_id,
            event_id,
            workspace,
            author,
            title,
            body,
            created_at,
            updated_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $7)
        ",
    )
    .bind(epic_id)
    .bind(event_id)
    .bind(&request.workspace)
    .bind(&request.author)
    .bind(&request.title)
    .bind(&request.body)
    .bind(created_at)
    .execute(&mut *tx)
    .await?;
    let record = EpicRecord::from(fetch_epic_by_id_tx(&mut tx, epic_id, &request.workspace).await?);
    let receipt =
        complete_idempotent_command(&mut tx, pending_receipt.as_ref(), &record, created_at).await?;
    tx.commit().await?;
    project_graph_event()
        .pool(state.pool())
        .projection_coordinator(state.projection_coordinator())
        .workspace(&record.workspace)
        .event_id(record.event_id)
        .operation(Box::pin(async {
            project_epic(state.graph(), &record)
                .await
                .map_err(ThreadplaneServerError::internal)
        }))
        .call()
        .await?;

    Ok(success_with_receipt(record, receipt))
}

pub(crate) async fn create_memory(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateMemoryRequest>,
) -> AppResult<MemoryRecord> {
    require_workspace_editor()
        .pool(state.pool())
        .bootstrap(state.bootstrap())
        .workspace(&request.workspace)
        .actor(&request.author)
        .call()
        .await?;
    let mut tx = state.pool().begin().await?;
    let memory_id = Uuid::new_v4();
    let created_at = Utc::now();
    let payload = serde_json::to_value(&request)?;
    let normalized_tags = normalize_memory_tags(request.tags.clone());
    let normalized_recall_triggers =
        normalize_memory_recall_triggers(request.recall_triggers.clone());
    let pending_receipt = match begin_idempotent_command::<MemoryRecord>(
        &mut tx,
        IdempotencyContext {
            actor: &request.author,
            command_kind: "create_memory",
            idempotency_key: idempotency_key(&headers)?,
            request_payload: &payload,
            workspace: &request.workspace,
        },
        created_at,
    )
    .await?
    {
        CommandExecution::Execute(pending_receipt) => pending_receipt,
        CommandExecution::Replay(envelope) => {
            tx.commit().await?;
            return Ok(Json(envelope));
        }
    };
    let event_id = append_event(
        &mut tx,
        &request.workspace,
        &request.author,
        EventKind::MemoryRecorded,
        &payload,
        created_at,
    )
    .await?;

    sqlx::query(
        "
        INSERT INTO memories (
            memory_id,
            event_id,
            workspace,
            author,
            title,
            body,
            kind,
            scope,
            audience,
            importance,
            tags,
            recall_triggers,
            created_at,
            updated_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $13)
        ",
    )
    .bind(memory_id)
    .bind(event_id)
    .bind(&request.workspace)
    .bind(&request.author)
    .bind(&request.title)
    .bind(&request.body)
    .bind(request.kind.as_str())
    .bind(request.scope.to_string())
    .bind(request.audience.to_string())
    .bind(request.importance.to_string())
    .bind(&normalized_tags)
    .bind(&normalized_recall_triggers)
    .bind(created_at)
    .execute(&mut *tx)
    .await?;
    let record = MemoryRecord::try_from(
        fetch_memory_by_id_tx(&mut tx, memory_id, &request.workspace).await?,
    )?;
    let receipt =
        complete_idempotent_command(&mut tx, pending_receipt.as_ref(), &record, created_at).await?;
    tx.commit().await?;
    project_graph_event()
        .pool(state.pool())
        .projection_coordinator(state.projection_coordinator())
        .workspace(&record.workspace)
        .event_id(record.event_id)
        .operation(Box::pin(project_memory_record(state.graph(), &record)))
        .call()
        .await?;

    Ok(success_with_receipt(record, receipt))
}

pub(crate) async fn create_note(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateNoteRequest>,
) -> AppResult<NoteRecord> {
    require_workspace_editor()
        .pool(state.pool())
        .bootstrap(state.bootstrap())
        .workspace(&request.workspace)
        .actor(&request.author)
        .call()
        .await?;
    let mut tx = state.pool().begin().await?;
    let note_id = Uuid::new_v4();
    let created_at = Utc::now();
    let payload = serde_json::to_value(&request)?;
    let pending_receipt = match begin_idempotent_command::<NoteRecord>(
        &mut tx,
        IdempotencyContext {
            actor: &request.author,
            command_kind: "create_note",
            idempotency_key: idempotency_key(&headers)?,
            request_payload: &payload,
            workspace: &request.workspace,
        },
        created_at,
    )
    .await?
    {
        CommandExecution::Execute(pending_receipt) => pending_receipt,
        CommandExecution::Replay(envelope) => {
            tx.commit().await?;
            return Ok(Json(envelope));
        }
    };
    let event_id = append_event(
        &mut tx,
        &request.workspace,
        &request.author,
        EventKind::NoteRecorded,
        &payload,
        created_at,
    )
    .await?;

    sqlx::query(
        "
        INSERT INTO notes (
            note_id,
            event_id,
            workspace,
            author,
            title,
            body,
            transclusion_id,
            created_at,
            updated_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, NULL, $7, $7)
        ",
    )
    .bind(note_id)
    .bind(event_id)
    .bind(&request.workspace)
    .bind(&request.author)
    .bind(&request.title)
    .bind(&request.body)
    .bind(created_at)
    .execute(&mut *tx)
    .await?;
    let record = NoteRecord::from(fetch_note_by_id_tx(&mut tx, note_id, &request.workspace).await?);
    let receipt =
        complete_idempotent_command(&mut tx, pending_receipt.as_ref(), &record, created_at).await?;
    tx.commit().await?;
    project_graph_event()
        .pool(state.pool())
        .projection_coordinator(state.projection_coordinator())
        .workspace(&record.workspace)
        .event_id(record.event_id)
        .operation(Box::pin(async {
            project_note(state.graph(), &record).await.map_err(|error| {
                error!(?error, note_id = %record.note_id, "failed to project note");
                ThreadplaneServerError::internal(error)
            })
        }))
        .call()
        .await?;

    Ok(success_with_receipt(record, receipt))
}

pub(crate) async fn update_note(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<UpdateNoteRequest>,
) -> AppResult<NoteRecord> {
    require_workspace_editor()
        .pool(state.pool())
        .bootstrap(state.bootstrap())
        .workspace(&request.workspace)
        .actor(&request.actor)
        .call()
        .await?;
    let mut tx = state.pool().begin().await?;
    let updated_at = Utc::now();
    let payload = serde_json::to_value(&request)?;
    let pending_receipt = match begin_idempotent_command::<NoteRecord>(
        &mut tx,
        IdempotencyContext {
            actor: &request.actor,
            command_kind: "update_note",
            idempotency_key: idempotency_key(&headers)?,
            request_payload: &payload,
            workspace: &request.workspace,
        },
        updated_at,
    )
    .await?
    {
        CommandExecution::Execute(pending_receipt) => pending_receipt,
        CommandExecution::Replay(envelope) => {
            tx.commit().await?;
            return Ok(Json(envelope));
        }
    };
    let note = fetch_note_by_id_tx(&mut tx, request.note_id, &request.workspace).await?;
    let event_id = append_event(
        &mut tx,
        &request.workspace,
        &request.actor,
        EventKind::NoteUpdated,
        &payload,
        updated_at,
    )
    .await?;

    let transclusion_id = if let Some(transclusion_id) = note.transclusion_id {
        update_transclusion_group(
            &mut tx,
            transclusion_id,
            &request.workspace,
            &request.actor,
            &request.title,
            &request.body,
            updated_at,
        )
        .await?;
        sync_transclusion_members(&mut tx, transclusion_id).await?;
        Some(transclusion_id)
    } else {
        sqlx::query(
            "
            UPDATE notes
            SET title = $2,
                body = $3,
                updated_at = $4
            WHERE note_id = $1
            ",
        )
        .bind(request.note_id)
        .bind(&request.title)
        .bind(&request.body)
        .bind(updated_at)
        .execute(&mut *tx)
        .await?;
        None
    };
    let record =
        NoteRecord::from(fetch_note_by_id_tx(&mut tx, request.note_id, &request.workspace).await?);
    let receipt =
        complete_idempotent_command(&mut tx, pending_receipt.as_ref(), &record, updated_at).await?;
    tx.commit().await?;

    project_graph_event()
        .pool(state.pool())
        .projection_coordinator(state.projection_coordinator())
        .workspace(&request.workspace)
        .event_id(event_id)
        .operation(Box::pin(async {
            if let Some(group_id) = transclusion_id {
                reproject_transclusion_group(state.graph(), state.pool(), group_id, None)
                    .await
                    .map_err(ThreadplaneServerError::internal)
            } else {
                project_note(state.graph(), &record)
                    .await
                    .map_err(ThreadplaneServerError::internal)
            }
        }))
        .call()
        .await?;
    Ok(success_with_receipt(record, receipt))
}

pub(crate) async fn add_link(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<AddLinkRequest>,
) -> AppResult<LinkRecord> {
    require_workspace_editor()
        .pool(state.pool())
        .bootstrap(state.bootstrap())
        .workspace(&request.workspace)
        .actor(&request.actor)
        .call()
        .await?;
    let mut tx = state.pool().begin().await?;
    let created_at = Utc::now();
    let payload = serde_json::to_value(&request)?;
    let pending_receipt = match begin_idempotent_command::<LinkRecord>(
        &mut tx,
        IdempotencyContext {
            actor: &request.actor,
            command_kind: "add_link",
            idempotency_key: idempotency_key(&headers)?,
            request_payload: &payload,
            workspace: &request.workspace,
        },
        created_at,
    )
    .await?
    {
        CommandExecution::Execute(pending_receipt) => pending_receipt,
        CommandExecution::Replay(envelope) => {
            tx.commit().await?;
            return Ok(Json(envelope));
        }
    };
    let event_id = append_event(
        &mut tx,
        &request.workspace,
        &request.actor,
        EventKind::LinkDeclared,
        &payload,
        created_at,
    )
    .await?;

    let link_id = Uuid::new_v4();
    sqlx::query(
        "
        INSERT INTO links (
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
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, false, NULL, $8)
        ",
    )
    .bind(link_id)
    .bind(event_id)
    .bind(&request.workspace)
    .bind(&request.actor)
    .bind(&request.from)
    .bind(&request.to)
    .bind(&request.relation)
    .bind(created_at)
    .execute(&mut *tx)
    .await?;
    let record = LinkRecord {
        link_id,
        event_id,
        workspace: request.workspace,
        actor: request.actor,
        from: request.from,
        to: request.to,
        relation: request.relation,
        is_xanadu: false,
        transclusion_id: None,
        created_at: created_at.to_rfc3339(),
    };
    let receipt =
        complete_idempotent_command(&mut tx, pending_receipt.as_ref(), &record, created_at).await?;
    tx.commit().await?;

    project_graph_event()
        .pool(state.pool())
        .projection_coordinator(state.projection_coordinator())
        .workspace(&record.workspace)
        .event_id(record.event_id)
        .operation(Box::pin(async {
            project_link(state.graph(), &record).await.map_err(|error| {
                error!(?error, link_id = %record.link_id, "failed to project link");
                ThreadplaneServerError::internal(error)
            })
        }))
        .call()
        .await?;

    Ok(success_with_receipt(record, receipt))
}

pub(crate) async fn add_xanadu_link(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateXanaduLinkRequest>,
) -> AppResult<LinkRecord> {
    require_workspace_editor()
        .pool(state.pool())
        .bootstrap(state.bootstrap())
        .workspace(&request.workspace)
        .actor(&request.actor)
        .call()
        .await?;
    let mut tx = state.pool().begin().await?;
    let created_at = Utc::now();
    let request_payload = build_xanadu_request_payload(&request);
    let pending_receipt = match begin_idempotent_command::<LinkRecord>(
        &mut tx,
        IdempotencyContext {
            actor: &request.actor,
            command_kind: "add_xanadu_link",
            idempotency_key: idempotency_key(&headers)?,
            request_payload: &request_payload,
            workspace: &request.workspace,
        },
        created_at,
    )
    .await?
    {
        CommandExecution::Execute(pending_receipt) => pending_receipt,
        CommandExecution::Replay(envelope) => {
            tx.commit().await?;
            return Ok(Json(envelope));
        }
    };
    let xanadu_group = prepare_xanadu_group(&mut tx, &request, created_at).await?;
    let event_payload = build_xanadu_event_payload(
        &request,
        xanadu_group.canonical_group_id,
        xanadu_group.merged_group_id,
    );
    let event_id = append_event(
        &mut tx,
        &request.workspace,
        &request.actor,
        EventKind::XanaduLinked,
        &event_payload,
        created_at,
    )
    .await?;
    let record = persist_xanadu_link()
        .tx(&mut tx)
        .request(&request)
        .event_id(event_id)
        .canonical_group_id(xanadu_group.canonical_group_id)
        .created_at(created_at)
        .call()
        .await?;
    let receipt =
        complete_idempotent_command(&mut tx, pending_receipt.as_ref(), &record, created_at).await?;
    tx.commit().await?;

    project_graph_event()
        .pool(state.pool())
        .projection_coordinator(state.projection_coordinator())
        .workspace(&record.workspace)
        .event_id(record.event_id)
        .operation(Box::pin(async {
            reproject_transclusion_group(
                state.graph(),
                state.pool(),
                xanadu_group.canonical_group_id,
                xanadu_group.merged_group_id,
            )
            .await
            .map_err(ThreadplaneServerError::internal)
        }))
        .call()
        .await?;

    Ok(success_with_receipt(record, receipt))
}

pub(crate) async fn update_workspace_policy(
    State(state): State<AppState>,
    Path(WorkspacePath { workspace }): Path<WorkspacePath>,
    Json(request): Json<UpdateWorkspacePolicyRequest>,
) -> AppResult<WorkspacePolicy> {
    if request.workspace != workspace {
        return Err(ThreadplaneServerError::bad_request(
            "request workspace must match the path workspace",
        ));
    }

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
    if request.workspace != workspace {
        return Err(ThreadplaneServerError::bad_request(
            "request workspace must match the path workspace",
        ));
    }

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
    if request.workspace != workspace {
        return Err(ThreadplaneServerError::bad_request(
            "request workspace must match the path workspace",
        ));
    }

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

#[inline]
#[must_use]
pub(crate) fn normalized_list_limit(limit: Option<i64>) -> i64 {
    limit.unwrap_or(DEFAULT_LIST_LIMIT).clamp(1, MAX_LIST_LIMIT)
}
