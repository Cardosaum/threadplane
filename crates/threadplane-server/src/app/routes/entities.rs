use axum::{routing::get, Router};

use crate::{
    app::AppState,
    handlers::{related_entities, show_entity},
};

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route("/{entity_ref}", get(show_entity))
        .route("/{entity_ref}/relations", get(related_entities))
}
