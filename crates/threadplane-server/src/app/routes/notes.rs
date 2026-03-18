use axum::{
    routing::{get, post},
    Router,
};

use crate::{
    app::AppState,
    handlers::{create_note, show_note, update_note},
};

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route("/", post(create_note))
        .nest("/{note_id}", member_routes())
}

fn member_routes() -> Router<AppState> {
    Router::new().route("/", get(show_note).patch(update_note))
}
