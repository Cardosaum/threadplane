#![allow(
    clippy::wildcard_imports,
    reason = "The link handler submodule intentionally builds on the handler prelude."
)]

use super::super::*;

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
