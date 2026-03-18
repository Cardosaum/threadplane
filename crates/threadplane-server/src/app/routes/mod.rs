use axum::{routing::get, Router};

use crate::{
    app::AppState,
    handlers::{healthz, root, scope},
};

mod entities;
mod epics;
mod links;
mod memories;
mod notes;
mod projections;
mod tasks;
mod workspaces;

pub(super) fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/", get(root))
        .route("/healthz", get(healthz))
        .route("/scope", get(scope))
        .nest("/v1", api_v1_router())
        .with_state(state)
}

fn api_v1_router() -> Router<AppState> {
    Router::new()
        .nest("/entities", entities::router())
        .nest("/epics", epics::router())
        .nest("/links", links::router())
        .nest("/memories", memories::router())
        .nest("/notes", notes::router())
        .nest("/projections", projections::router())
        .nest("/tasks", tasks::router())
        .nest("/workspaces/{workspace}", workspaces::router())
}
