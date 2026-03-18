use alloc::sync::Arc;
use core::net::SocketAddr;

use axum::serve;
use neo4rs::{query, Graph};
use snafu::ResultExt as _;
use sqlx::{postgres::PgPoolOptions, PgPool};
use tokio::{net::TcpListener, task::JoinHandle};
use tokio_util::sync::CancellationToken;
use tracing::{error, info};
use tracing_subscriber::{fmt, layer::SubscriberExt as _, util::SubscriberInitExt as _};

use super::{AppConfig, AppDependencies, AppState, LeasePolicy, ProjectionCoordinator};
use crate::error::{BindListener, ConnectNeo4j, ConnectPostgres, LoadConfig, Serve, VerifyNeo4j};
use crate::{
    error::ServerResult,
    lifecycle::{wait_for_shutdown, watch_for_shutdown_signal},
    migration::run_migrations,
    replay::{catch_up_graph_projection, spawn_graph_projection_worker, GRAPH_PROJECTION_NAME},
};
use threadplane_core::{load_threadplane_config, SERVICE_NAME};

pub(crate) async fn run() -> ServerResult<()> {
    init_tracing();

    let config = AppConfig::from_runtime_config(load_threadplane_config().context(LoadConfig)?)?;
    let shutdown = ShutdownCoordinator::new();
    let runtime = ServerRuntime::bootstrap(config).await?;
    let run_result = Box::pin(runtime.run(shutdown.token())).await;
    shutdown.shutdown().await;
    run_result
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

        let app = super::build_router(self.state.clone());
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
