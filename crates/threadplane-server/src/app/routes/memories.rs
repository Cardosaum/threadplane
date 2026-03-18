use axum::{
    routing::{get, post},
    Router,
};

use crate::{
    app::AppState,
    handlers::{create_memory, show_memory},
};

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route("/", post(create_memory))
        .nest("/{memory_id}", member_routes())
}

fn member_routes() -> Router<AppState> {
    Router::new().route("/", get(show_memory))
}
