#![allow(
    clippy::wildcard_imports,
    reason = "The link handler submodule intentionally builds on the handler prelude."
)]

use super::super::*;

pub(crate) fn build_xanadu_request_payload(request: &CreateXanaduLinkRequest) -> Value {
    json!({
        "workspace": request.workspace,
        "actor": request.actor,
        "from": request.from,
        "to": request.to,
        "transclusion_id": null,
        "merged_group_id": null,
    })
}

pub(crate) fn build_xanadu_event_payload(
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
pub(crate) async fn persist_xanadu_link(
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
