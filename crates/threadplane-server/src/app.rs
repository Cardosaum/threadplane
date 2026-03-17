#![expect(
    clippy::arbitrary_source_item_ordering,
    reason = "App bootstrap groups lifecycle and wiring by layer rather than alphabetically."
)]
#![expect(
    clippy::redundant_pub_crate,
    reason = "Server app types are shared only across crate-local modules."
)]

use alloc::sync::Arc;
use core::future::Future;
use core::net::SocketAddr;

use axum::{
    routing::{get, post},
    serve, Router,
};
use derive_more::Constructor;
use neo4rs::{query, Graph};
use snafu::ResultExt as _;
use sqlx::{postgres::PgPoolOptions, PgPool};
use tokio::{net::TcpListener, sync::Mutex, task::JoinHandle};
use tokio_util::sync::CancellationToken;
use tracing::{error, info};
use tracing_subscriber::{fmt, layer::SubscriberExt as _, util::SubscriberInitExt as _};

use crate::error::{
    BindListener, ConnectNeo4j, ConnectPostgres, InvalidBindAddress,
    InvalidWorkspaceBootstrap, LoadConfig, Serve, VerifyNeo4j,
};
use crate::{
    error::ServerResult,
    handlers::{
        add_link, add_task_dependency, add_workspace_public_key, add_xanadu_link,
        claim_next_task, claim_task, complete_task, create_epic, create_note,
        grant_workspace_membership, healthz, list_epics, list_events, list_notes,
        list_open_tasks, list_tasks, list_workspace_memberships, list_workspace_public_keys,
        next_task, offer_task, projection_status, related_entities, release_task, root, scope,
        show_entity, show_epic, show_note, show_task, show_workspace_policy, tail_events,
        task_context, task_dag, update_note, update_task, update_workspace_policy,
    },
    lifecycle::{wait_for_shutdown, watch_for_shutdown_signal},
    migration::run_migrations,
    replay::{catch_up_graph_projection, spawn_graph_projection_worker, GRAPH_PROJECTION_NAME},
};
use threadplane_core::{
    load_threadplane_config, validate_workspace_policy, ActorPublicKey, PublicKeyAlgorithm,
    ThreadplaneConfig, WorkspaceAuthPolicy, WorkspaceBootstrapConfig, WorkspaceMembership,
    WorkspacePolicy, WorkspacePriorityPolicy, WorkspaceRole, SERVICE_NAME,
};

#[derive(Clone, Constructor)]
pub(crate) struct AppState {
    bootstrap: WorkspaceGovernanceBootstrap,
    dependencies: AppDependencies,
    lease_policy: LeasePolicy,
    projection_coordinator: ProjectionCoordinator,
}

impl AppState {
    pub(crate) const fn bootstrap(&self) -> &WorkspaceGovernanceBootstrap {
        &self.bootstrap
    }

    pub(crate) const fn default_lease_seconds(&self) -> i64 {
        self.lease_policy.default_lease_seconds()
    }

    pub(crate) fn graph(&self) -> &Graph {
        self.dependencies.graph()
    }

    pub(crate) const fn pool(&self) -> &PgPool {
        self.dependencies.pool()
    }

    pub(crate) async fn shutdown(&self) {
        self.dependencies.shutdown().await;
    }

    pub(crate) async fn serialize_graph_projection<T, Operation>(
        &self,
        operation: Operation,
    ) -> ServerResult<T>
    where
        Operation: Future<Output = ServerResult<T>>,
    {
        self.projection_coordinator.run(operation).await
    }
}

#[derive(Clone, Constructor)]
pub(crate) struct AppDependencies {
    graph: Arc<Graph>,
    pool: PgPool,
}

impl AppDependencies {
    fn graph(&self) -> &Graph {
        self.graph.as_ref()
    }

    const fn pool(&self) -> &PgPool {
        &self.pool
    }

    async fn shutdown(&self) {
        self.pool.close().await;
    }
}

#[derive(Clone, Copy, Constructor)]
pub(crate) struct LeasePolicy {
    default_lease_seconds: i64,
}

impl LeasePolicy {
    const fn default_lease_seconds(self) -> i64 {
        self.default_lease_seconds
    }
}

#[derive(Clone, Default)]
pub(crate) struct ProjectionCoordinator {
    gate: Arc<Mutex<()>>,
}

impl ProjectionCoordinator {
    pub(crate) async fn run<T, Operation>(&self, operation: Operation) -> ServerResult<T>
    where
        Operation: Future<Output = ServerResult<T>>,
    {
        let _guard = self.gate.lock().await;
        operation.await
    }
}

#[derive(Debug, Clone, Constructor)]
pub(crate) struct BootstrapMembership {
    actor_id: String,
    role: WorkspaceRole,
}

#[derive(Debug, Clone, Constructor)]
pub(crate) struct BootstrapPublicKey {
    actor_id: String,
    algorithm: PublicKeyAlgorithm,
    key_id: String,
    public_key: String,
}

#[derive(Debug, Clone, Constructor)]
pub(crate) struct WorkspaceGovernanceBootstrap {
    auth: WorkspaceAuthPolicy,
    memberships: Vec<BootstrapMembership>,
    priorities: WorkspacePriorityPolicy,
    public_keys: Vec<BootstrapPublicKey>,
}

impl WorkspaceGovernanceBootstrap {
    fn from_config(config: WorkspaceBootstrapConfig) -> ServerResult<Self> {
        validate_workspace_policy(&WorkspacePolicy {
            auth: config.auth.clone(),
            priorities: config.priorities.clone(),
            workspace: "__bootstrap__".to_owned(),
        })
        .map_err(|error| InvalidWorkspaceBootstrap {
            reason: error.to_string(),
        }
        .build())?;

        Ok(Self::new(
            config.auth,
            config
                .memberships
                .into_iter()
                .map(|membership| BootstrapMembership::new(membership.actor_id, membership.role))
                .collect(),
            config.priorities,
            config
                .public_keys
                .into_iter()
                .map(|key| {
                    BootstrapPublicKey::new(key.actor_id, key.algorithm, key.key_id, key.public_key)
                })
                .collect(),
        ))
    }

    pub(crate) fn memberships_for_workspace(&self, workspace: &str) -> Vec<WorkspaceMembership> {
        self.memberships
            .iter()
            .map(|membership| WorkspaceMembership {
                actor_id: membership.actor_id.clone(),
                role: membership.role,
                workspace: workspace.to_owned(),
            })
            .collect()
    }

    pub(crate) fn policy_for_workspace(&self, workspace: &str) -> WorkspacePolicy {
        WorkspacePolicy {
            auth: self.auth.clone(),
            priorities: self.priorities.clone(),
            workspace: workspace.to_owned(),
        }
    }

    pub(crate) fn public_keys(&self) -> Vec<ActorPublicKey> {
        self.public_keys
            .iter()
            .map(|key| ActorPublicKey {
                actor_id: key.actor_id.clone(),
                algorithm: key.algorithm,
                key_id: key.key_id.clone(),
                public_key: key.public_key.clone(),
            })
            .collect()
    }
}

struct ServerRuntime {
    bind_addr: SocketAddr,
    listener: TcpListener,
    state: AppState,
}

impl ServerRuntime {
    async fn bootstrap(config: AppConfig) -> ServerResult<Self> {
        info!("bootstrapping server runtime");
        let dependencies = connect_dependencies(&config).await?;
        info!(bind_addr = %config.bind_addr, "connected external dependencies");
        let listener = bind_listener(config.bind_addr).await?;
        info!(bind_addr = %config.bind_addr, "bound tcp listener");
        let lease_policy = LeasePolicy::new(config.default_lease_seconds);
        let projection_coordinator = ProjectionCoordinator::default();
        let state = AppState::new(
            config.workspace_bootstrap,
            dependencies,
            lease_policy,
            projection_coordinator,
        );

        Ok(Self {
            bind_addr: config.bind_addr,
            listener,
            state,
        })
    }

    async fn run(self, shutdown_token: CancellationToken) -> ServerResult<()> {
        let (projection_shutdown, projection_worker) =
            Box::pin(self.start_projection_worker(&shutdown_token)).await?;
        info!(service = SERVICE_NAME, bind_addr = %self.bind_addr, "server listening");

        let app = build_router(self.state.clone());
        let serve_result = serve(self.listener, app)
            .with_graceful_shutdown(wait_for_shutdown(shutdown_token))
            .await
            .context(Serve);
        stop_projection_worker(projection_shutdown, projection_worker).await;
        self.state.shutdown().await;
        serve_result
    }

    async fn start_projection_worker(
        &self,
        shutdown_token: &CancellationToken,
    ) -> ServerResult<(CancellationToken, JoinHandle<()>)> {
        info!(
            projection = GRAPH_PROJECTION_NAME,
            "catching up graph projection before serving"
        );
        let replayed = Box::pin(catch_up_graph_projection(&self.state)).await?;
        info!(
            projection = GRAPH_PROJECTION_NAME,
            replayed, "caught up graph projection before serving"
        );

        let projection_shutdown = shutdown_token.clone();
        let projection_worker =
            spawn_graph_projection_worker(self.state.clone(), projection_shutdown.clone());
        Ok((projection_shutdown, projection_worker))
    }
}

async fn stop_projection_worker(
    projection_shutdown: CancellationToken,
    projection_worker: JoinHandle<()>,
) {
    projection_shutdown.cancel();
    if let Err(error) = projection_worker.await {
        if !error.is_cancelled() {
            error!(?error, "projection worker terminated unexpectedly");
        }
    }
}

struct ShutdownCoordinator {
    signal_task: JoinHandle<()>,
    token: CancellationToken,
}

impl ShutdownCoordinator {
    fn new() -> Self {
        let token = CancellationToken::new();
        let signal_task = tokio::spawn(watch_for_shutdown_signal(token.clone()));

        Self { signal_task, token }
    }

    fn token(&self) -> CancellationToken {
        self.token.clone()
    }

    async fn shutdown(self) {
        self.token.cancel();

        if let Err(error) = self.signal_task.await {
            error!(?error, "shutdown signal task terminated unexpectedly");
        }
    }
}

pub(crate) struct AppConfig {
    bind_addr: SocketAddr,
    database_url: String,
    default_lease_seconds: i64,
    neo4j_uri: String,
    neo4j_user: String,
    neo4j_password: String,
    workspace_bootstrap: WorkspaceGovernanceBootstrap,
}

impl AppConfig {
    fn from_runtime_config() -> ServerResult<Self> {
        let config = load_threadplane_config().context(LoadConfig)?;
        Self::from_threadplane_config(config)
    }

    fn from_threadplane_config(config: ThreadplaneConfig) -> ServerResult<Self> {
        let bind_addr = config.server.bind.parse().context(InvalidBindAddress {
            value: config.server.bind.clone(),
        })?;
        let workspace_bootstrap =
            WorkspaceGovernanceBootstrap::from_config(config.server.workspace_bootstrap)?;

        Ok(Self {
            bind_addr,
            database_url: config.server.database_url,
            default_lease_seconds: config.server.default_lease_seconds,
            neo4j_uri: config.server.neo4j_uri,
            neo4j_user: config.server.neo4j_user,
            neo4j_password: config.server.neo4j_password,
            workspace_bootstrap,
        })
    }
}

fn build_router(state: AppState) -> Router {
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
        .route("/notes", get(list_notes))
        .route("/policy", get(show_workspace_policy).put(update_workspace_policy))
        .route("/memberships", get(list_workspace_memberships).post(grant_workspace_membership))
        .route("/keys", get(list_workspace_public_keys).post(add_workspace_public_key))
        .nest("/tasks", workspace_task_routes())
}

fn workspace_task_routes() -> Router<AppState> {
    Router::new()
        .route("/", get(list_tasks))
        .route("/next", get(next_task))
        .route("/open", get(list_open_tasks))
}

async fn bind_listener(bind_addr: SocketAddr) -> ServerResult<TcpListener> {
    TcpListener::bind(bind_addr)
        .await
        .context(BindListener { bind_addr })
}

async fn connect_dependencies(config: &AppConfig) -> ServerResult<AppDependencies> {
    let pool = connect_postgres_with_migrations(config).await?;
    let graph = connect_graph_projection(config).await?;
    info!("connected postgres and neo4j");
    Ok(AppDependencies::new(graph, pool))
}

async fn connect_graph_projection(config: &AppConfig) -> ServerResult<Arc<Graph>> {
    info!("connecting neo4j");
    connect_neo4j(
        &config.neo4j_uri,
        &config.neo4j_user,
        &config.neo4j_password,
    )
    .await
}

async fn connect_postgres_with_migrations(config: &AppConfig) -> ServerResult<PgPool> {
    info!("connecting postgres");
    let pool = connect_postgres(&config.database_url).await?;
    info!("running postgres migrations");
    run_migrations(&pool).await?;
    Ok(pool)
}

async fn connect_neo4j(
    neo4j_uri: &str,
    neo4j_user: &str,
    neo4j_password: &str,
) -> ServerResult<Arc<Graph>> {
    let graph = Arc::new(
        Graph::new(neo4j_uri, neo4j_user, neo4j_password)
            .await
            .context(ConnectNeo4j {
                neo4j_uri: neo4j_uri.to_owned(),
            })?,
    );
    graph.run(query("RETURN 1")).await.context(VerifyNeo4j)?;
    Ok(graph)
}

async fn connect_postgres(database_url: &str) -> ServerResult<PgPool> {
    PgPoolOptions::new()
        .max_connections(10)
        .connect(database_url)
        .await
        .context(ConnectPostgres {
            database_url: database_url.to_owned(),
        })
}

fn init_tracing() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "threadplane_server=info".into()),
        )
        .with(fmt::layer())
        .init();
}

pub(crate) async fn run() -> ServerResult<()> {
    init_tracing();

    let config = AppConfig::from_runtime_config()?;
    let shutdown = ShutdownCoordinator::new();
    let runtime = ServerRuntime::bootstrap(config).await?;
    let run_result = Box::pin(runtime.run(shutdown.token())).await;
    shutdown.shutdown().await;
    run_result
}
