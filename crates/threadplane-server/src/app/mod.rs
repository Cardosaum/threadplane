#![expect(
    clippy::arbitrary_source_item_ordering,
    reason = "App bootstrap groups lifecycle and wiring by layer rather than alphabetically."
)]
#![expect(
    clippy::redundant_pub_crate,
    reason = "Server app types are shared only across crate-local modules."
)]

use core::future::Future;

use alloc::sync::Arc;

use axum::extract::FromRef;
use derive_more::Constructor;
use neo4rs::Graph;
use sqlx::PgPool;
use tokio::sync::Mutex;

use crate::error::ServerResult;

mod config;
mod routes;
mod runtime;

use self::config::AppConfig;
pub(crate) use self::config::WorkspaceGovernanceBootstrap;
use self::routes::build_router;
pub(crate) use self::runtime::run;

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

    pub(crate) const fn projection_coordinator(&self) -> &ProjectionCoordinator {
        &self.projection_coordinator
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

impl FromRef<AppState> for Arc<Graph> {
    #[inline]
    fn from_ref(input: &AppState) -> Self {
        Self::clone(&input.dependencies.graph)
    }
}

impl FromRef<AppState> for PgPool {
    #[inline]
    fn from_ref(input: &AppState) -> Self {
        input.dependencies.pool.clone()
    }
}

impl FromRef<AppState> for ProjectionCoordinator {
    #[inline]
    fn from_ref(input: &AppState) -> Self {
        input.projection_coordinator.clone()
    }
}

impl FromRef<AppState> for WorkspaceGovernanceBootstrap {
    #[inline]
    fn from_ref(input: &AppState) -> Self {
        input.bootstrap.clone()
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
