use super::*;

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
