use axum::{
    routing::{get, post},
    Router,
};

use crate::{
    app::AppState,
    handlers::{create_epic, show_epic},
};

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route("/", post(create_epic))
        .nest("/{epic_id}", member_routes())
}

fn member_routes() -> Router<AppState> {
    Router::new().route("/", get(show_epic))
}
