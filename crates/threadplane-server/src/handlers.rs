#![expect(
    clippy::redundant_pub_crate,
    reason = "Handlers are crate-local endpoints with explicit visibility."
)]

use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    Json,
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{json, Value};
use tracing::error;
use uuid::Uuid;

use crate::{
    app::AppState,
    build_info::current_build_info,
    error::{AppResult, ServerResult, ThreadplaneServerError},
    idempotency::{
        begin_idempotent_command, complete_idempotent_command, CommandExecution,
        IdempotencyContext, IDEMPOTENCY_KEY_HEADER,
    },
    lifecycle::{calculate_claim_expiry, normalized_lease_seconds},
    projections::{
        fetch_task_relations, project_claim, project_epic, project_link, project_note,
        project_task, project_task_dependency_by_id, project_task_supporting_entities,
        reproject_transclusion_group,
    },
    replay::GRAPH_PROJECTION_NAME,
    storage::{
        append_event, append_task_dependency, build_task_list_entries, fetch_active_claim,
        fetch_active_claim_tx, fetch_dependency_chain, fetch_dependent_chain,
        fetch_direct_dependencies, fetch_direct_dependents, fetch_epic_by_id, fetch_epic_by_id_tx,
        fetch_epic_for_task, fetch_epic_rows_for_workspace, fetch_event_rows_for_workspace,
        fetch_note_by_id, fetch_note_by_id_tx, fetch_projection_status, fetch_task_by_id,
        fetch_task_by_id_tx, fetch_tasks_for_listing, prepare_xanadu_group,
        sync_transclusion_members, task_is_ready, unique_task_ids, update_transclusion_group,
        TaskListFilters,
    },
};
use threadplane_core::{
    health_summary, normalize_task_labels, normalize_task_owner, scope_summary,
    service_snapshot, AddLinkRequest, AddTaskDependencyRequest, ApiEnvelope, ClaimTaskRequest,
    CompleteTaskRequest, CreateEpicRequest, CreateNoteRequest, CreateXanaduLinkRequest,
    EpicRecord, EventKind, EventRecord, LinkRecord, NoteRecord, OfferTaskRequest,
    ProjectionStatus, ReleaseTaskRequest, ServiceSnapshot, TaskClaimRecord, TaskContext, TaskDag,
    TaskListEntry, TaskPriority, TaskRecord, UpdateNoteRequest, UpdateTaskRequest,
    DEPENDS_ON_RELATION, XANADU_RELATION,
};

const DEFAULT_LIST_LIMIT: i64 = 25;
const MAX_LIST_LIMIT: i64 = 200;

#[derive(Debug, Deserialize)]
pub(crate) struct ListQuery {
    limit: Option<i64>,
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

fn with_projection_status(
    mut payload: Value,
    projection: ProjectionStatus,
) -> Result<Value, serde_json::Error> {
    if let Value::Object(ref mut object) = payload {
        object.insert(
            "projection".to_owned(),
            serde_json::to_value(projection)?,
        );
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

async fn persist_task_update(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    request: &UpdateTaskRequest,
    current_transclusion_id: Option<Uuid>,
    updated_at: DateTime<Utc>,
) -> ServerResult<Option<Uuid>> {
    if let Some(transclusion_id) = current_transclusion_id {
        update_transclusion_group(
            tx,
            transclusion_id,
            &request.workspace,
            &request.actor,
            &request.title,
            &request.details,
            updated_at,
        )
        .await?;
        sync_transclusion_members(tx, transclusion_id).await?;
        sqlx::query(
            "
            UPDATE tasks
            SET epic_id = COALESCE($2, epic_id),
                priority = $3,
                owner = $4,
                labels = $5,
                updated_at = $6
            WHERE task_id = $1
            ",
        )
        .bind(request.task_id)
        .bind(request.epic_id)
        .bind(request.metadata.priority.to_string())
        .bind(request.metadata.owner.clone())
        .bind(request.metadata.labels.clone())
        .bind(updated_at)
        .execute(&mut **tx)
        .await?;
        return Ok(Some(transclusion_id));
    }

    sqlx::query(
        "
        UPDATE tasks
        SET title = $2,
            details = $3,
            epic_id = COALESCE($4, epic_id),
            priority = $5,
            owner = $6,
            labels = $7,
            updated_at = $8
        WHERE task_id = $1
        ",
    )
    .bind(request.task_id)
    .bind(&request.title)
    .bind(&request.details)
    .bind(request.epic_id)
    .bind(request.metadata.priority.to_string())
    .bind(request.metadata.owner.clone())
    .bind(request.metadata.labels.clone())
    .bind(updated_at)
    .execute(&mut **tx)
    .await?;

    Ok(None)
}

async fn ensure_task_is_unclaimed(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    task_id: Uuid,
) -> ServerResult<()> {
    let active_claim = fetch_active_claim_tx(tx, task_id).await?;
    if let Some(claim) = active_claim {
        return Err(ThreadplaneServerError::conflict(format!(
            "task already claimed by {} until {}",
            claim.actor,
            claim.expires_at.to_rfc3339()
        )));
    }

    Ok(())
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

pub(crate) async fn root() -> Json<ServiceSnapshot> {
    Json(service_snapshot(current_build_info()))
}

pub(crate) async fn healthz(State(state): State<AppState>) -> ServerResult<Json<Value>> {
    let projection = fetch_projection_status(state.pool(), GRAPH_PROJECTION_NAME).await?;
    Ok(Json(with_projection_status(
        health_summary(&current_build_info()),
        projection,
    )?))
}

pub(crate) async fn scope(State(state): State<AppState>) -> ServerResult<Json<Value>> {
    let projection = fetch_projection_status(state.pool(), GRAPH_PROJECTION_NAME).await?;
    Ok(Json(with_projection_status(
        scope_summary(&current_build_info()),
        projection,
    )?))
}

pub(crate) async fn projection_status(State(state): State<AppState>) -> AppResult<ProjectionStatus> {
    let data = fetch_projection_status(state.pool(), GRAPH_PROJECTION_NAME).await?;
    Ok(success(data))
}

pub(crate) async fn create_epic(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateEpicRequest>,
) -> AppResult<EpicRecord> {
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
        complete_idempotent_command(&mut tx, pending_receipt.as_ref(), &record, created_at)
            .await?;
    tx.commit().await?;
    project_epic(state.graph(), &record)
        .await
        .map_err(ThreadplaneServerError::internal)?;

    Ok(success_with_receipt(record, receipt))
}

pub(crate) async fn show_epic(
    State(state): State<AppState>,
    Path(epic_id): Path<Uuid>,
) -> AppResult<EpicRecord> {
    let row = fetch_epic_by_id(state.pool(), epic_id).await?;
    Ok(success(EpicRecord::from(row)))
}

pub(crate) async fn create_note(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateNoteRequest>,
) -> AppResult<NoteRecord> {
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
        complete_idempotent_command(&mut tx, pending_receipt.as_ref(), &record, created_at)
            .await?;
    tx.commit().await?;
    project_note(state.graph(), &record)
        .await
        .map_err(|error| {
            error!(?error, note_id = %record.note_id, "failed to project note");
            ThreadplaneServerError::internal(error)
        })?;

    Ok(success_with_receipt(record, receipt))
}

pub(crate) async fn show_note(
    State(state): State<AppState>,
    Path(note_id): Path<Uuid>,
) -> AppResult<NoteRecord> {
    let row = fetch_note_by_id(state.pool(), note_id).await?;
    Ok(success(NoteRecord::from(row)))
}

pub(crate) async fn update_note(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<UpdateNoteRequest>,
) -> AppResult<NoteRecord> {
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
    append_event(
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
    let record = NoteRecord::from(fetch_note_by_id_tx(&mut tx, request.note_id, &request.workspace).await?);
    let receipt =
        complete_idempotent_command(&mut tx, pending_receipt.as_ref(), &record, updated_at)
            .await?;
    tx.commit().await?;

    if let Some(group_id) = transclusion_id {
        reproject_transclusion_group(&state, group_id, None)
            .await
            .map_err(ThreadplaneServerError::internal)?;
    } else {
        project_note(state.graph(), &record)
            .await
            .map_err(ThreadplaneServerError::internal)?;
    }
    Ok(success_with_receipt(record, receipt))
}

pub(crate) async fn offer_task(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(mut request): Json<OfferTaskRequest>,
) -> AppResult<TaskRecord> {
    request.metadata.labels = normalize_task_labels(request.metadata.labels);
    request.metadata.owner = normalize_task_owner(request.metadata.owner);
    let mut tx = state.pool().begin().await?;
    if let Some(epic_id) = request.epic_id {
        fetch_epic_by_id_tx(&mut tx, epic_id, &request.workspace).await?;
    }
    let task_id = Uuid::new_v4();
    let created_at = Utc::now();
    let payload = serde_json::to_value(&request)?;
    let pending_receipt = match begin_idempotent_command::<TaskRecord>(
        &mut tx,
        IdempotencyContext {
            actor: &request.author,
            command_kind: "offer_task",
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
        EventKind::TaskOffered,
        &payload,
        created_at,
    )
    .await?;

    sqlx::query(
        "
        INSERT INTO tasks (
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
            updated_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, 'open', $7, $8, $9, $10, NULL, $11, $11)
        ",
    )
    .bind(task_id)
    .bind(event_id)
    .bind(&request.workspace)
    .bind(&request.author)
    .bind(&request.title)
    .bind(&request.details)
    .bind(request.epic_id)
    .bind(request.metadata.priority.to_string())
    .bind(request.metadata.owner.clone())
    .bind(request.metadata.labels.clone())
    .bind(created_at)
    .execute(&mut *tx)
    .await?;

    for dependency_id in unique_task_ids(&request.depends_on) {
        append_task_dependency(
            &mut tx,
            &request.workspace,
            &request.author,
            task_id,
            dependency_id,
            created_at,
        )
        .await?;
    }
    let record = TaskRecord::from(fetch_task_by_id_tx(&mut tx, task_id, &request.workspace).await?);
    let receipt =
        complete_idempotent_command(&mut tx, pending_receipt.as_ref(), &record, created_at)
            .await?;
    tx.commit().await?;
    project_task_supporting_entities(&state, &record).await?;
    project_task(state.graph(), &record)
        .await
        .map_err(|error| {
            error!(?error, task_id = %record.task_id, "failed to project task");
            ThreadplaneServerError::internal(error)
        })?;

    Ok(success_with_receipt(record, receipt))
}

pub(crate) async fn update_task(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(mut request): Json<UpdateTaskRequest>,
) -> AppResult<TaskRecord> {
    request.metadata.labels = normalize_task_labels(request.metadata.labels);
    request.metadata.owner = normalize_task_owner(request.metadata.owner);
    let mut tx = state.pool().begin().await?;
    let updated_at = Utc::now();
    let payload = serde_json::to_value(&request)?;
    let pending_receipt = match begin_idempotent_command::<TaskRecord>(
        &mut tx,
        IdempotencyContext {
            actor: &request.actor,
            command_kind: "update_task",
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
    let task = fetch_task_by_id_tx(&mut tx, request.task_id, &request.workspace).await?;
    if let Some(epic_id) = request.epic_id {
        fetch_epic_by_id_tx(&mut tx, epic_id, &request.workspace).await?;
    }
    append_event(
        &mut tx,
        &request.workspace,
        &request.actor,
        EventKind::TaskUpdated,
        &payload,
        updated_at,
    )
    .await?;

    let transclusion_id =
        persist_task_update(&mut tx, &request, task.transclusion_id, updated_at).await?;
    let record = TaskRecord::from(fetch_task_by_id_tx(&mut tx, request.task_id, &request.workspace).await?);
    let receipt =
        complete_idempotent_command(&mut tx, pending_receipt.as_ref(), &record, updated_at)
            .await?;
    tx.commit().await?;

    if let Some(group_id) = transclusion_id {
        reproject_transclusion_group(&state, group_id, None)
            .await
            .map_err(ThreadplaneServerError::internal)?;
    } else {
        project_task(state.graph(), &record)
            .await
            .map_err(ThreadplaneServerError::internal)?;
    }
    project_task_supporting_entities(&state, &record).await?;
    Ok(success_with_receipt(record, receipt))
}

pub(crate) async fn claim_task(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<ClaimTaskRequest>,
) -> AppResult<TaskClaimRecord> {
    let lease_seconds =
        normalized_lease_seconds(request.lease_seconds, state.default_lease_seconds());
    let mut tx = state.pool().begin().await?;
    let claimed_at = Utc::now();
    let payload = serde_json::to_value(&request)?;
    let pending_receipt = match begin_idempotent_command::<TaskClaimRecord>(
        &mut tx,
        IdempotencyContext {
            actor: &request.actor,
            command_kind: "claim_task",
            idempotency_key: idempotency_key(&headers)?,
            request_payload: &payload,
            workspace: &request.workspace,
        },
        claimed_at,
    )
    .await?
    {
        CommandExecution::Execute(pending_receipt) => pending_receipt,
        CommandExecution::Replay(envelope) => {
            tx.commit().await?;
            return Ok(Json(envelope));
        }
    };

    fetch_task_by_id_tx(&mut tx, request.task_id, &request.workspace).await?;

    ensure_task_is_unclaimed(&mut tx, request.task_id).await?;
    let expires_at = calculate_claim_expiry(claimed_at, lease_seconds)
        .ok_or_else(|| ThreadplaneServerError::bad_request("lease expiration overflow"))?;
    let event_id = append_event(
        &mut tx,
        &request.workspace,
        &request.actor,
        EventKind::TaskClaimed,
        &payload,
        claimed_at,
    )
    .await?;

    let claim_id = Uuid::new_v4();
    sqlx::query(
        "
        INSERT INTO task_claims (claim_id, task_id, workspace, actor, event_id, claimed_at, expires_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        ",
    )
    .bind(claim_id)
    .bind(request.task_id)
    .bind(&request.workspace)
    .bind(&request.actor)
    .bind(event_id)
    .bind(claimed_at)
    .bind(expires_at)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "
        UPDATE tasks
        SET status = 'claimed',
            updated_at = $2
        WHERE task_id = $1
        ",
    )
    .bind(request.task_id)
    .bind(claimed_at)
    .execute(&mut *tx)
    .await?;
    let workspace = request.workspace.clone();
    let actor = request.actor.clone();
    let record = TaskClaimRecord {
        claim_id,
        task_id: request.task_id,
        workspace,
        actor,
        event_id,
        claimed_at: claimed_at.to_rfc3339(),
        expires_at: expires_at.to_rfc3339(),
    };
    let receipt =
        complete_idempotent_command(&mut tx, pending_receipt.as_ref(), &record, claimed_at)
            .await?;
    let task = fetch_task_by_id_tx(&mut tx, request.task_id, &request.workspace).await?;
    tx.commit().await?;
    let task_record = TaskRecord::from(task.clone());
    project_task_supporting_entities(&state, &task_record).await?;
    project_task(state.graph(), &task_record)
        .await
        .map_err(ThreadplaneServerError::internal)?;
    project_claim(state.graph(), &task, &record)
        .await
        .map_err(|error| {
            error!(?error, task_id = %record.task_id, "failed to project claim");
            ThreadplaneServerError::internal(error)
        })?;

    Ok(success_with_receipt(record, receipt))
}

pub(crate) async fn release_task(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<ReleaseTaskRequest>,
) -> AppResult<TaskRecord> {
    let mut tx = state.pool().begin().await?;
    let released_at = Utc::now();
    let payload = serde_json::to_value(&request)?;
    let pending_receipt = match begin_idempotent_command::<TaskRecord>(
        &mut tx,
        IdempotencyContext {
            actor: &request.actor,
            command_kind: "release_task",
            idempotency_key: idempotency_key(&headers)?,
            request_payload: &payload,
            workspace: &request.workspace,
        },
        released_at,
    )
    .await?
    {
        CommandExecution::Execute(pending_receipt) => pending_receipt,
        CommandExecution::Replay(envelope) => {
            tx.commit().await?;
            return Ok(Json(envelope));
        }
    };
    fetch_task_by_id_tx(&mut tx, request.task_id, &request.workspace).await?;
    let claim = fetch_active_claim_tx(&mut tx, request.task_id)
        .await?
        .ok_or_else(|| {
            ThreadplaneServerError::conflict("task does not have an active claim to release")
        })?;

    if claim.actor != request.actor {
        return Err(ThreadplaneServerError::conflict(format!(
            "task is currently claimed by {}",
            claim.actor
        )));
    }
    append_event(
        &mut tx,
        &request.workspace,
        &request.actor,
        EventKind::TaskReleased,
        &payload,
        released_at,
    )
    .await?;

    sqlx::query(
        "
        UPDATE task_claims
        SET released_at = $2
        WHERE claim_id = $1
        ",
    )
    .bind(claim.claim_id)
    .bind(released_at)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "
        UPDATE tasks
        SET status = 'open',
            updated_at = $2
        WHERE task_id = $1
        ",
    )
    .bind(request.task_id)
    .bind(released_at)
    .execute(&mut *tx)
    .await?;
    let record = TaskRecord::from(fetch_task_by_id_tx(&mut tx, request.task_id, &request.workspace).await?);
    let receipt =
        complete_idempotent_command(&mut tx, pending_receipt.as_ref(), &record, released_at)
            .await?;
    tx.commit().await?;
    project_task_supporting_entities(&state, &record).await?;
    project_task(state.graph(), &record)
        .await
        .map_err(ThreadplaneServerError::internal)?;

    Ok(success_with_receipt(record, receipt))
}

pub(crate) async fn complete_task(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CompleteTaskRequest>,
) -> AppResult<TaskRecord> {
    let mut tx = state.pool().begin().await?;
    let completed_at = Utc::now();
    let payload = serde_json::to_value(&request)?;
    let pending_receipt = match begin_idempotent_command::<TaskRecord>(
        &mut tx,
        IdempotencyContext {
            actor: &request.actor,
            command_kind: "complete_task",
            idempotency_key: idempotency_key(&headers)?,
            request_payload: &payload,
            workspace: &request.workspace,
        },
        completed_at,
    )
    .await?
    {
        CommandExecution::Execute(pending_receipt) => pending_receipt,
        CommandExecution::Replay(envelope) => {
            tx.commit().await?;
            return Ok(Json(envelope));
        }
    };

    if let Some(claim) = fetch_active_claim_tx(&mut tx, request.task_id).await? {
        if claim.actor != request.actor {
            return Err(ThreadplaneServerError::conflict(format!(
                "task is currently claimed by {}",
                claim.actor
            )));
        }
    }
    append_event(
        &mut tx,
        &request.workspace,
        &request.actor,
        EventKind::TaskCompleted,
        &payload,
        completed_at,
    )
    .await?;

    sqlx::query(
        "
        UPDATE task_claims
        SET released_at = $2
        WHERE task_id = $1
          AND released_at IS NULL
          AND expires_at > now()
        ",
    )
    .bind(request.task_id)
    .bind(completed_at)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "
        UPDATE tasks
        SET status = 'completed',
            updated_at = $2
        WHERE task_id = $1
        ",
    )
    .bind(request.task_id)
    .bind(completed_at)
    .execute(&mut *tx)
    .await?;
    let record = TaskRecord::from(fetch_task_by_id_tx(&mut tx, request.task_id, &request.workspace).await?);
    let receipt =
        complete_idempotent_command(&mut tx, pending_receipt.as_ref(), &record, completed_at)
            .await?;
    tx.commit().await?;
    project_task_supporting_entities(&state, &record).await?;
    project_task(state.graph(), &record)
        .await
        .map_err(ThreadplaneServerError::internal)?;

    Ok(success_with_receipt(record, receipt))
}

pub(crate) async fn add_task_dependency(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<AddTaskDependencyRequest>,
) -> AppResult<Value> {
    let mut tx = state.pool().begin().await?;
    let created_at = Utc::now();
    let payload = serde_json::to_value(&request)?;
    let pending_receipt = match begin_idempotent_command::<Value>(
        &mut tx,
        IdempotencyContext {
            actor: &request.actor,
            command_kind: "add_task_dependency",
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
    append_task_dependency(
        &mut tx,
        &request.workspace,
        &request.actor,
        request.task_id,
        request.depends_on_task_id,
        created_at,
    )
    .await?;
    let data = json!({
        "task_id": request.task_id,
        "depends_on_task_id": request.depends_on_task_id,
        "relation": DEPENDS_ON_RELATION,
    });
    let receipt =
        complete_idempotent_command(&mut tx, pending_receipt.as_ref(), &data, created_at).await?;
    tx.commit().await?;

    project_task_dependency_by_id(
        state.graph(),
        state.pool(),
        request.task_id,
        request.depends_on_task_id,
    )
    .await
    .map_err(ThreadplaneServerError::internal)?;

    Ok(success_with_receipt(data, receipt))
}

pub(crate) async fn add_link(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<AddLinkRequest>,
) -> AppResult<LinkRecord> {
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
        complete_idempotent_command(&mut tx, pending_receipt.as_ref(), &record, created_at)
            .await?;
    tx.commit().await?;

    project_link(state.graph(), &record)
        .await
        .map_err(|error| {
            error!(?error, link_id = %record.link_id, "failed to project link");
            ThreadplaneServerError::internal(error)
        })?;

    Ok(success_with_receipt(record, receipt))
}

pub(crate) async fn add_xanadu_link(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateXanaduLinkRequest>,
) -> AppResult<LinkRecord> {
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
    .bind(xanadu_group.canonical_group_id)
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
        relation: XANADU_RELATION.to_owned(),
        is_xanadu: true,
        transclusion_id: Some(xanadu_group.canonical_group_id),
        created_at: created_at.to_rfc3339(),
    };
    let receipt =
        complete_idempotent_command(&mut tx, pending_receipt.as_ref(), &record, created_at)
            .await?;
    tx.commit().await?;

    reproject_transclusion_group(
        &state,
        xanadu_group.canonical_group_id,
        xanadu_group.merged_group_id,
    )
    .await
    .map_err(ThreadplaneServerError::internal)?;

    Ok(success_with_receipt(record, receipt))
}

pub(crate) async fn list_events(
    State(state): State<AppState>,
    Path(workspace): Path<String>,
    Query(query): Query<ListQuery>,
) -> AppResult<Vec<EventRecord>> {
    let limit = normalized_list_limit(query.limit);
    let rows = fetch_event_rows_for_workspace(state.pool(), &workspace, limit).await?;
    let data = rows.into_iter().map(EventRecord::from).collect();
    Ok(success(data))
}

pub(crate) async fn list_epics(
    State(state): State<AppState>,
    Path(workspace): Path<String>,
) -> AppResult<Vec<EpicRecord>> {
    let rows = fetch_epic_rows_for_workspace(state.pool(), &workspace).await?;
    let data = rows.into_iter().map(EpicRecord::from).collect();
    Ok(success(data))
}

pub(crate) async fn list_tasks(
    State(state): State<AppState>,
    Path(workspace): Path<String>,
    Query(query): Query<TaskListQuery>,
) -> AppResult<Vec<TaskListEntry>> {
    let limit = normalized_list_limit(query.limit);
    let rows = fetch_tasks_for_listing(
        state.pool(),
        &workspace,
        TaskListFilters {
            epic_id: query.epic_id,
            label: query.label.as_deref(),
            owner: query.owner.as_deref(),
            priority: query.priority,
            ready_only: query.ready_only.unwrap_or(false),
            status: query.status.as_deref(),
        },
    )
    .await?;
    let limited_rows = truncate_results(rows, limit);
    let data = build_task_list_entries(state.pool(), limited_rows).await?;
    Ok(success(data))
}

pub(crate) async fn list_open_tasks(
    State(state): State<AppState>,
    Path(workspace): Path<String>,
) -> AppResult<Vec<TaskListEntry>> {
    let rows = fetch_tasks_for_listing(
        state.pool(),
        &workspace,
        TaskListFilters {
            ready_only: false,
            status: Some("open"),
            ..TaskListFilters::default()
        },
    )
    .await?;
    let data = build_task_list_entries(state.pool(), rows).await?;
    Ok(success(data))
}

pub(crate) async fn show_task(
    State(state): State<AppState>,
    Path(task_id): Path<Uuid>,
) -> AppResult<TaskRecord> {
    let data = TaskRecord::from(fetch_task_by_id(state.pool(), task_id).await?);
    Ok(success(data))
}

pub(crate) async fn task_context(
    State(state): State<AppState>,
    Path(task_id): Path<Uuid>,
) -> AppResult<TaskContext> {
    let task = fetch_task_by_id(state.pool(), task_id).await?;
    let active_claim = fetch_active_claim(state.pool(), task_id)
        .await?
        .map(TaskClaimRecord::from);
    let epic = fetch_epic_for_task(state.pool(), &task).await?;
    let dependencies = fetch_direct_dependencies(state.pool(), task_id).await?;
    let dependents = fetch_direct_dependents(state.pool(), task_id).await?;
    let relations = fetch_task_relations(state.graph(), task_id)
        .await
        .map_err(ThreadplaneServerError::internal)?;

    let data = TaskContext {
        task: task.clone().into(),
        active_claim,
        dependencies,
        dependents,
        epic,
        ready: task_is_ready(state.pool(), task_id).await?,
        relations,
    };

    Ok(success(data))
}

pub(crate) async fn task_dag(
    State(state): State<AppState>,
    Path(task_id): Path<Uuid>,
) -> AppResult<TaskDag> {
    let task = fetch_task_by_id(state.pool(), task_id).await?;
    let data = TaskDag {
        task: task.clone().into(),
        epic: fetch_epic_for_task(state.pool(), &task).await?,
        ready: task_is_ready(state.pool(), task_id).await?,
        dependencies: fetch_dependency_chain(state.pool(), task_id).await?,
        dependents: fetch_dependent_chain(state.pool(), task_id).await?,
    };

    Ok(success(data))
}

#[inline]
#[must_use]
pub(crate) fn normalized_list_limit(limit: Option<i64>) -> i64 {
    limit.unwrap_or(DEFAULT_LIST_LIMIT).clamp(1, MAX_LIST_LIMIT)
}

fn truncate_results<T>(items: Vec<T>, limit: i64) -> Vec<T> {
    let max_items = usize::try_from(limit).map_or(usize::MAX, |value| value);

    items.into_iter().take(max_items).collect()
}
