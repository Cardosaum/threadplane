#![allow(
    clippy::wildcard_imports,
    reason = "Read handlers use the parent handler module as their import boundary."
)]

use super::super::*;

pub(crate) async fn root() -> Json<ServiceSnapshot> {
    Json(service_snapshot(current_build_info()))
}

pub(crate) async fn healthz(State(pool): State<PgPool>) -> ServerResult<Json<Value>> {
    let projection = fetch_projection_status(&pool, GRAPH_PROJECTION_NAME).await?;
    Ok(Json(with_projection_status(
        health_summary(&current_build_info()),
        projection,
    )?))
}

pub(crate) async fn scope(State(pool): State<PgPool>) -> ServerResult<Json<Value>> {
    let projection = fetch_projection_status(&pool, GRAPH_PROJECTION_NAME).await?;
    Ok(Json(with_projection_status(
        scope_summary(&current_build_info()),
        projection,
    )?))
}

pub(crate) async fn projection_status(State(pool): State<PgPool>) -> AppResult<ProjectionStatus> {
    let data = fetch_projection_status(&pool, GRAPH_PROJECTION_NAME).await?;
    Ok(success(data))
}
