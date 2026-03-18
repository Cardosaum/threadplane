use axum::{http::HeaderMap, Json};
use bon::builder;
use chrono::Utc;
use core::future::Future;
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    idempotency::IDEMPOTENCY_KEY_HEADER,
    prelude::*,
    replay::GRAPH_PROJECTION_NAME,
    storage::{
        ensure_workspace_governance, fetch_event_row_for_workspace, record_projection_cursor,
        require_workspace_role,
    },
};

use threadplane_core::{ApiEnvelope, ProjectionStatus, WorkspacePolicy, WorkspaceRole};

const DEFAULT_LIST_LIMIT: i64 = 25;
const MAX_LIST_LIMIT: i64 = 200;

pub(crate) fn with_projection_status(
    mut payload: Value,
    projection: ProjectionStatus,
) -> Result<Value, serde_json::Error> {
    if let Value::Object(ref mut object) = payload {
        object.insert("projection".to_owned(), serde_json::to_value(projection)?);
    }

    Ok(payload)
}

pub(crate) const fn success<T>(data: T) -> Json<ApiEnvelope<T>> {
    Json(ApiEnvelope {
        data,
        ok: true,
        receipt: None,
    })
}

pub(crate) const fn success_with_receipt<T>(
    data: T,
    receipt: Option<threadplane_core::CommandReceipt>,
) -> Json<ApiEnvelope<T>> {
    Json(ApiEnvelope {
        data,
        ok: true,
        receipt,
    })
}

pub(crate) fn idempotency_key(headers: &HeaderMap) -> ServerResult<Option<&str>> {
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

pub(crate) async fn ensure_workspace_policy(
    pool: &PgPool,
    bootstrap: &WorkspaceGovernanceBootstrap,
    workspace: &str,
) -> ServerResult<WorkspacePolicy> {
    ensure_workspace_governance(pool, workspace, bootstrap).await
}

#[builder]
pub(crate) async fn require_workspace_editor(
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
pub(crate) async fn require_workspace_admin(
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

pub(crate) async fn advance_graph_projection_cursor(
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
pub(crate) async fn project_graph_event<Output, Operation>(
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
