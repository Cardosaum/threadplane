use axum::{routing::get, Router};

use crate::{app::AppState, handlers::projection_status};

pub(super) fn router() -> Router<AppState> {
    Router::new().route("/graph", get(projection_status))
}
