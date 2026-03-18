use axum::{
    routing::{get, post},
    Router,
};

use crate::{
    app::AppState,
    handlers::{
        add_task_dependency, claim_next_task, claim_task, complete_task, offer_task, release_task,
        show_task, task_context, task_dag, update_task,
    },
};

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route("/", post(offer_task))
        .nest("/claims", claim_routes())
        .nest("/{task_id}", member_routes())
}

fn claim_routes() -> Router<AppState> {
    Router::new().route("/next", post(claim_next_task))
}

fn member_routes() -> Router<AppState> {
    Router::new()
        .route("/", get(show_task).patch(update_task))
        .route("/claims", post(claim_task))
        .route("/claims/release", post(release_task))
        .route("/completion", post(complete_task))
        .route("/context", get(task_context))
        .route("/dag", get(task_dag))
        .route("/dependencies", post(add_task_dependency))
}
