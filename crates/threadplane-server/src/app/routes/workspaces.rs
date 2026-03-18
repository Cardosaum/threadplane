use axum::{routing::get, Router};

use crate::{
    app::AppState,
    handlers::{
        add_workspace_public_key, grant_workspace_membership, list_epics, list_events,
        list_memories, list_notes, list_open_tasks, list_tasks, list_workspace_memberships,
        list_workspace_public_keys, next_task, prime_memories, show_workspace_policy, tail_events,
        update_workspace_policy,
    },
};

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route("/epics", get(list_epics))
        .route("/events", get(list_events))
        .route("/events/tail", get(tail_events))
        .route("/memories", get(list_memories))
        .route("/memories/prime", get(prime_memories))
        .route("/notes", get(list_notes))
        .route(
            "/policy",
            get(show_workspace_policy).put(update_workspace_policy),
        )
        .route(
            "/memberships",
            get(list_workspace_memberships).post(grant_workspace_membership),
        )
        .route(
            "/keys",
            get(list_workspace_public_keys).post(add_workspace_public_key),
        )
        .nest("/tasks", task_routes())
}

fn task_routes() -> Router<AppState> {
    Router::new()
        .route("/", get(list_tasks))
        .route("/next", get(next_task))
        .route("/open", get(list_open_tasks))
}
