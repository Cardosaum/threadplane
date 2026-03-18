#![allow(
    clippy::wildcard_imports,
    reason = "The link handler submodule intentionally builds on the handler prelude."
)]

use super::{super::*, shared::*};

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
