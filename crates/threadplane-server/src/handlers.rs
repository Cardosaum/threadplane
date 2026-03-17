#![expect(
    clippy::redundant_pub_crate,
    reason = "Handlers are crate-local endpoints with explicit visibility."
)]

use axum::{
    extract::{Path, Query, State},
    Json,
};
use chrono::Utc;
use serde::Deserialize;
use serde_json::{json, Value};
use tracing::error;
use uuid::Uuid;

use crate::{
    app::AppState,
    build_info::current_build_info,
    error::{AppResult, ThreadplaneServerError},
    lifecycle::{calculate_claim_expiry, normalized_lease_seconds},
    projections::{
        fetch_task_relations, project_claim, project_epic, project_link, project_note,
        project_task, project_task_dependency_by_id, project_task_supporting_entities,
        reproject_transclusion_group,
    },
    storage::{
        append_event, append_task_dependency, build_task_list_entries, fetch_active_claim,
        fetch_active_claim_tx, fetch_dependency_chain, fetch_dependent_chain,
        fetch_direct_dependencies, fetch_direct_dependents, fetch_epic_by_id, fetch_epic_by_id_tx,
        fetch_epic_for_task, fetch_epic_rows_for_workspace, fetch_event_rows_for_workspace,
        fetch_note_by_id, fetch_note_by_id_tx, fetch_task_by_id, fetch_task_by_id_tx,
        fetch_tasks_for_listing, prepare_xanadu_group, sync_transclusion_members, task_is_ready,
        unique_task_ids, update_transclusion_group,
    },
};
use threadplane_core::{
    health_summary, scope_summary, service_snapshot, AddLinkRequest, AddTaskDependencyRequest,
    ApiEnvelope, ClaimTaskRequest, CompleteTaskRequest, CreateEpicRequest, CreateNoteRequest,
    CreateXanaduLinkRequest, EpicRecord, EventKind, EventRecord, LinkRecord, NoteRecord,
    OfferTaskRequest, ReleaseTaskRequest, ServiceSnapshot, TaskClaimRecord, TaskContext, TaskDag,
    TaskListEntry, TaskRecord, UpdateNoteRequest, UpdateTaskRequest, DEPENDS_ON_RELATION,
    XANADU_RELATION,
};

#[derive(Debug, Deserialize)]
pub(crate) struct ListQuery {
    limit: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct TaskListQuery {
    epic_id: Option<Uuid>,
    ready_only: Option<bool>,
    status: Option<String>,
}

pub(crate) async fn root() -> Json<ServiceSnapshot> {
    Json(service_snapshot(current_build_info()))
}

pub(crate) async fn healthz() -> Json<Value> {
    Json(health_summary(&current_build_info()))
}

pub(crate) async fn scope() -> Json<Value> {
    Json(scope_summary(&current_build_info()))
}

pub(crate) async fn create_epic(
    State(state): State<AppState>,
    Json(request): Json<CreateEpicRequest>,
) -> AppResult<EpicRecord> {
    let mut tx = state.pool().begin().await?;
    let epic_id = Uuid::new_v4();
    let created_at = Utc::now();
    let payload = serde_json::to_value(&request)?;
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

    tx.commit().await?;

    let row = fetch_epic_by_id(state.pool(), epic_id).await?;
    let record = EpicRecord::from(row);
    project_epic(state.graph(), &record)
        .await
        .map_err(ThreadplaneServerError::internal)?;

    Ok(Json(ApiEnvelope {
        ok: true,
        data: record,
    }))
}

pub(crate) async fn show_epic(
    State(state): State<AppState>,
    Path(epic_id): Path<Uuid>,
) -> AppResult<EpicRecord> {
    let row = fetch_epic_by_id(state.pool(), epic_id).await?;
    Ok(Json(ApiEnvelope {
        ok: true,
        data: EpicRecord::from(row),
    }))
}

pub(crate) async fn create_note(
    State(state): State<AppState>,
    Json(request): Json<CreateNoteRequest>,
) -> AppResult<NoteRecord> {
    let mut tx = state.pool().begin().await?;
    let note_id = Uuid::new_v4();
    let created_at = Utc::now();
    let payload = serde_json::to_value(&request)?;
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

    tx.commit().await?;

    let row = fetch_note_by_id(state.pool(), note_id).await?;
    let record = NoteRecord::from(row);
    project_note(state.graph(), &record)
        .await
        .map_err(|error| {
            error!(?error, note_id = %record.note_id, "failed to project note");
            ThreadplaneServerError::internal(error)
        })?;

    Ok(Json(ApiEnvelope {
        ok: true,
        data: record,
    }))
}

pub(crate) async fn show_note(
    State(state): State<AppState>,
    Path(note_id): Path<Uuid>,
) -> AppResult<NoteRecord> {
    let row = fetch_note_by_id(state.pool(), note_id).await?;
    Ok(Json(ApiEnvelope {
        ok: true,
        data: NoteRecord::from(row),
    }))
}

pub(crate) async fn update_note(
    State(state): State<AppState>,
    Json(request): Json<UpdateNoteRequest>,
) -> AppResult<NoteRecord> {
    let mut tx = state.pool().begin().await?;
    let note = fetch_note_by_id_tx(&mut tx, request.note_id, &request.workspace).await?;
    let updated_at = Utc::now();
    let payload = serde_json::to_value(&request)?;
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

    tx.commit().await?;

    if let Some(group_id) = transclusion_id {
        reproject_transclusion_group(&state, group_id, None)
            .await
            .map_err(ThreadplaneServerError::internal)?;
    } else {
        let row = fetch_note_by_id(state.pool(), request.note_id).await?;
        project_note(state.graph(), &NoteRecord::from(row))
            .await
            .map_err(ThreadplaneServerError::internal)?;
    }

    let row = fetch_note_by_id(state.pool(), request.note_id).await?;
    Ok(Json(ApiEnvelope {
        ok: true,
        data: NoteRecord::from(row),
    }))
}

pub(crate) async fn offer_task(
    State(state): State<AppState>,
    Json(request): Json<OfferTaskRequest>,
) -> AppResult<TaskRecord> {
    let mut tx = state.pool().begin().await?;
    if let Some(epic_id) = request.epic_id {
        fetch_epic_by_id_tx(&mut tx, epic_id, &request.workspace).await?;
    }
    let task_id = Uuid::new_v4();
    let created_at = Utc::now();
    let payload = serde_json::to_value(&request)?;
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
            transclusion_id,
            created_at,
            updated_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, 'open', $7, NULL, $8, $8)
        ",
    )
    .bind(task_id)
    .bind(event_id)
    .bind(&request.workspace)
    .bind(&request.author)
    .bind(&request.title)
    .bind(&request.details)
    .bind(request.epic_id)
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

    tx.commit().await?;

    let row = fetch_task_by_id(state.pool(), task_id).await?;
    let record = TaskRecord::from(row);
    project_task_supporting_entities(&state, &record).await?;
    project_task(state.graph(), &record)
        .await
        .map_err(|error| {
            error!(?error, task_id = %record.task_id, "failed to project task");
            ThreadplaneServerError::internal(error)
        })?;

    Ok(Json(ApiEnvelope {
        ok: true,
        data: record,
    }))
}

pub(crate) async fn update_task(
    State(state): State<AppState>,
    Json(request): Json<UpdateTaskRequest>,
) -> AppResult<TaskRecord> {
    let mut tx = state.pool().begin().await?;
    let task = fetch_task_by_id_tx(&mut tx, request.task_id, &request.workspace).await?;
    if let Some(epic_id) = request.epic_id {
        fetch_epic_by_id_tx(&mut tx, epic_id, &request.workspace).await?;
    }
    let updated_at = Utc::now();
    let payload = serde_json::to_value(&request)?;
    append_event(
        &mut tx,
        &request.workspace,
        &request.actor,
        EventKind::TaskUpdated,
        &payload,
        updated_at,
    )
    .await?;

    let transclusion_id = if let Some(transclusion_id) = task.transclusion_id {
        update_transclusion_group(
            &mut tx,
            transclusion_id,
            &request.workspace,
            &request.actor,
            &request.title,
            &request.details,
            updated_at,
        )
        .await?;
        sync_transclusion_members(&mut tx, transclusion_id).await?;
        Some(transclusion_id)
    } else {
        sqlx::query(
            "
            UPDATE tasks
            SET title = $2,
                details = $3,
                epic_id = COALESCE($4, epic_id),
                updated_at = $5
            WHERE task_id = $1
            ",
        )
        .bind(request.task_id)
        .bind(&request.title)
        .bind(&request.details)
        .bind(request.epic_id)
        .bind(updated_at)
        .execute(&mut *tx)
        .await?;
        None
    };

    if transclusion_id.is_some() && request.epic_id.is_some() {
        sqlx::query(
            "
            UPDATE tasks
            SET epic_id = COALESCE($2, epic_id),
                updated_at = $3
            WHERE task_id = $1
            ",
        )
        .bind(request.task_id)
        .bind(request.epic_id)
        .bind(updated_at)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;

    if let Some(group_id) = transclusion_id {
        reproject_transclusion_group(&state, group_id, None)
            .await
            .map_err(ThreadplaneServerError::internal)?;
    } else {
        let row = fetch_task_by_id(state.pool(), request.task_id).await?;
        project_task(state.graph(), &TaskRecord::from(row))
            .await
            .map_err(ThreadplaneServerError::internal)?;
    }

    let row = fetch_task_by_id(state.pool(), request.task_id).await?;
    project_task_supporting_entities(&state, &TaskRecord::from(row.clone())).await?;
    Ok(Json(ApiEnvelope {
        ok: true,
        data: TaskRecord::from(row),
    }))
}

pub(crate) async fn claim_task(
    State(state): State<AppState>,
    Json(request): Json<ClaimTaskRequest>,
) -> AppResult<TaskClaimRecord> {
    let lease_seconds =
        normalized_lease_seconds(request.lease_seconds, state.default_lease_seconds());
    let mut tx = state.pool().begin().await?;

    fetch_task_by_id_tx(&mut tx, request.task_id, &request.workspace).await?;

    let active_claim = fetch_active_claim_tx(&mut tx, request.task_id).await?;
    if let Some(claim) = active_claim {
        return Err(ThreadplaneServerError::conflict(format!(
            "task already claimed by {} until {}",
            claim.actor,
            claim.expires_at.to_rfc3339()
        )));
    }

    let claimed_at = Utc::now();
    let expires_at = calculate_claim_expiry(claimed_at, lease_seconds)
        .ok_or_else(|| ThreadplaneServerError::bad_request("lease expiration overflow"))?;
    let payload = serde_json::to_value(&request)?;
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

    tx.commit().await?;

    let record = TaskClaimRecord {
        claim_id,
        task_id: request.task_id,
        workspace: request.workspace,
        actor: request.actor,
        event_id,
        claimed_at: claimed_at.to_rfc3339(),
        expires_at: expires_at.to_rfc3339(),
    };

    let task = fetch_task_by_id(state.pool(), request.task_id).await?;
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

    Ok(Json(ApiEnvelope {
        ok: true,
        data: record,
    }))
}

pub(crate) async fn release_task(
    State(state): State<AppState>,
    Json(request): Json<ReleaseTaskRequest>,
) -> AppResult<TaskRecord> {
    let mut tx = state.pool().begin().await?;
    fetch_task_by_id_tx(&mut tx, request.task_id, &request.workspace).await?;
    let released_at = Utc::now();
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

    let payload = serde_json::to_value(&request)?;
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

    tx.commit().await?;

    let task = fetch_task_by_id(state.pool(), request.task_id).await?;
    let record = TaskRecord::from(task);
    project_task_supporting_entities(&state, &record).await?;
    project_task(state.graph(), &record)
        .await
        .map_err(ThreadplaneServerError::internal)?;

    Ok(Json(ApiEnvelope {
        ok: true,
        data: record,
    }))
}

pub(crate) async fn complete_task(
    State(state): State<AppState>,
    Json(request): Json<CompleteTaskRequest>,
) -> AppResult<TaskRecord> {
    let mut tx = state.pool().begin().await?;
    let completed_at = Utc::now();

    if let Some(claim) = fetch_active_claim_tx(&mut tx, request.task_id).await? {
        if claim.actor != request.actor {
            return Err(ThreadplaneServerError::conflict(format!(
                "task is currently claimed by {}",
                claim.actor
            )));
        }
    }

    let payload = serde_json::to_value(&request)?;
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

    tx.commit().await?;

    let record = TaskRecord::from(fetch_task_by_id(state.pool(), request.task_id).await?);
    project_task_supporting_entities(&state, &record).await?;
    project_task(state.graph(), &record)
        .await
        .map_err(ThreadplaneServerError::internal)?;

    Ok(Json(ApiEnvelope {
        ok: true,
        data: record,
    }))
}

pub(crate) async fn add_task_dependency(
    State(state): State<AppState>,
    Json(request): Json<AddTaskDependencyRequest>,
) -> AppResult<Value> {
    let mut tx = state.pool().begin().await?;
    let created_at = Utc::now();
    append_task_dependency(
        &mut tx,
        &request.workspace,
        &request.actor,
        request.task_id,
        request.depends_on_task_id,
        created_at,
    )
    .await?;
    tx.commit().await?;

    project_task_dependency_by_id(
        state.graph(),
        state.pool(),
        request.task_id,
        request.depends_on_task_id,
    )
    .await
    .map_err(ThreadplaneServerError::internal)?;

    Ok(Json(ApiEnvelope {
        ok: true,
        data: json!({
            "task_id": request.task_id,
            "depends_on_task_id": request.depends_on_task_id,
            "relation": DEPENDS_ON_RELATION,
        }),
    }))
}

pub(crate) async fn add_link(
    State(state): State<AppState>,
    Json(request): Json<AddLinkRequest>,
) -> AppResult<LinkRecord> {
    let mut tx = state.pool().begin().await?;
    let created_at = Utc::now();
    let payload = serde_json::to_value(&request)?;
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

    tx.commit().await?;

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

    project_link(state.graph(), &record)
        .await
        .map_err(|error| {
            error!(?error, link_id = %record.link_id, "failed to project link");
            ThreadplaneServerError::internal(error)
        })?;

    Ok(Json(ApiEnvelope {
        ok: true,
        data: record,
    }))
}

pub(crate) async fn add_xanadu_link(
    State(state): State<AppState>,
    Json(request): Json<CreateXanaduLinkRequest>,
) -> AppResult<LinkRecord> {
    let mut tx = state.pool().begin().await?;
    let created_at = Utc::now();
    let xanadu_group = prepare_xanadu_group(&mut tx, &request, created_at).await?;

    let payload = json!({
        "workspace": request.workspace,
        "actor": request.actor,
        "from": request.from,
        "to": request.to,
        "transclusion_id": xanadu_group.canonical_group_id,
    });
    let event_id = append_event(
        &mut tx,
        &request.workspace,
        &request.actor,
        EventKind::XanaduLinked,
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

    tx.commit().await?;

    reproject_transclusion_group(
        &state,
        xanadu_group.canonical_group_id,
        xanadu_group.merged_group_id,
    )
    .await
    .map_err(ThreadplaneServerError::internal)?;

    Ok(Json(ApiEnvelope {
        ok: true,
        data: LinkRecord {
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
        },
    }))
}

pub(crate) async fn list_events(
    State(state): State<AppState>,
    Path(workspace): Path<String>,
    Query(query): Query<ListQuery>,
) -> AppResult<Vec<EventRecord>> {
    let limit = query.limit.unwrap_or(25).clamp(1, 200);
    let rows = fetch_event_rows_for_workspace(state.pool(), &workspace, limit).await?;
    let data = rows.into_iter().map(EventRecord::from).collect();
    Ok(Json(ApiEnvelope { ok: true, data }))
}

pub(crate) async fn list_epics(
    State(state): State<AppState>,
    Path(workspace): Path<String>,
) -> AppResult<Vec<EpicRecord>> {
    let rows = fetch_epic_rows_for_workspace(state.pool(), &workspace).await?;
    let data = rows.into_iter().map(EpicRecord::from).collect();
    Ok(Json(ApiEnvelope { ok: true, data }))
}

pub(crate) async fn list_tasks(
    State(state): State<AppState>,
    Path(workspace): Path<String>,
    Query(query): Query<TaskListQuery>,
) -> AppResult<Vec<TaskListEntry>> {
    let rows = fetch_tasks_for_listing(
        state.pool(),
        &workspace,
        query.status.as_deref(),
        query.epic_id,
        query.ready_only.unwrap_or(false),
    )
    .await?;
    let data = build_task_list_entries(state.pool(), rows).await?;
    Ok(Json(ApiEnvelope { ok: true, data }))
}

pub(crate) async fn list_open_tasks(
    State(state): State<AppState>,
    Path(workspace): Path<String>,
) -> AppResult<Vec<TaskListEntry>> {
    let rows = fetch_tasks_for_listing(state.pool(), &workspace, Some("open"), None, false).await?;
    let data = build_task_list_entries(state.pool(), rows).await?;
    Ok(Json(ApiEnvelope { ok: true, data }))
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

    Ok(Json(ApiEnvelope { ok: true, data }))
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

    Ok(Json(ApiEnvelope { ok: true, data }))
}
