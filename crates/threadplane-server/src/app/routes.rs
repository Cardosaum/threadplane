use axum::{
    routing::{get, post},
    Router,
};

use crate::{
    app::AppState,
    handlers::{
        add_link, add_task_dependency, add_workspace_public_key, add_xanadu_link, claim_next_task,
        claim_task, complete_task, create_epic, create_memory, create_note,
        grant_workspace_membership, healthz, list_epics, list_events, list_memories, list_notes,
        list_open_tasks, list_tasks, list_workspace_memberships, list_workspace_public_keys,
        next_task, offer_task, prime_memories, projection_status, related_entities, release_task,
        root, scope, show_entity, show_epic, show_memory, show_note, show_task,
        show_workspace_policy, tail_events, task_context, task_dag, update_note, update_task,
        update_workspace_policy,
    },
};

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
        .nest("/entities", entity_routes())
        .nest("/epics", epic_routes())
        .nest("/links", link_routes())
        .nest("/memories", memory_routes())
        .nest("/notes", note_routes())
        .nest("/projections", projection_routes())
        .nest("/tasks", task_routes())
        .nest("/workspaces/{workspace}", workspace_routes())
}

fn entity_routes() -> Router<AppState> {
    Router::new()
        .route("/{entity_ref}", get(show_entity))
        .route("/{entity_ref}/relations", get(related_entities))
}

fn epic_routes() -> Router<AppState> {
    Router::new()
        .route("/", post(create_epic))
        .nest("/{epic_id}", epic_member_routes())
}

fn epic_member_routes() -> Router<AppState> {
    Router::new().route("/", get(show_epic))
}

fn link_routes() -> Router<AppState> {
    Router::new()
        .route("/", post(add_link))
        .route("/xanadu", post(add_xanadu_link))
}

fn memory_routes() -> Router<AppState> {
    Router::new()
        .route("/", post(create_memory))
        .nest("/{memory_id}", memory_member_routes())
}

fn memory_member_routes() -> Router<AppState> {
    Router::new().route("/", get(show_memory))
}

fn note_routes() -> Router<AppState> {
    Router::new()
        .route("/", post(create_note))
        .nest("/{note_id}", note_member_routes())
}

fn note_member_routes() -> Router<AppState> {
    Router::new().route("/", get(show_note).patch(update_note))
}

fn projection_routes() -> Router<AppState> {
    Router::new().route("/graph", get(projection_status))
}

fn task_routes() -> Router<AppState> {
    Router::new()
        .route("/", post(offer_task))
        .nest("/claims", task_claim_routes())
        .nest("/{task_id}", task_member_routes())
}

fn task_claim_routes() -> Router<AppState> {
    Router::new().route("/next", post(claim_next_task))
}

fn task_member_routes() -> Router<AppState> {
    Router::new()
        .route("/", get(show_task).patch(update_task))
        .route("/claims", post(claim_task))
        .route("/claims/release", post(release_task))
        .route("/completion", post(complete_task))
        .route("/context", get(task_context))
        .route("/dag", get(task_dag))
        .route("/dependencies", post(add_task_dependency))
}

fn workspace_routes() -> Router<AppState> {
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
        .nest("/tasks", workspace_task_routes())
}

fn workspace_task_routes() -> Router<AppState> {
    Router::new()
        .route("/", get(list_tasks))
        .route("/next", get(next_task))
        .route("/open", get(list_open_tasks))
}
