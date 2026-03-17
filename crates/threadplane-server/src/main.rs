#![allow(
    clippy::arbitrary_source_item_ordering,
    clippy::too_many_lines,
    reason = "The server is intentionally a single-file POC entrypoint and reordering it wholesale would add noisy churn without improving behavior."
)]

extern crate alloc;

use alloc::sync::Arc;
use core::{
    fmt::Display,
    net::{AddrParseError, SocketAddr},
    result::Result as CoreResult,
    str::FromStr as _,
};
use std::io;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    serve, Json, Router,
};
use chrono::{DateTime, Duration, Utc};
use derive_more::Constructor;
use dotenvy::dotenv;
use neo4rs::{query, Graph};
use serde::Deserialize;
use serde_json::{json, Value};
use snafu::{IntoError as _, Location, ResultExt as _, Snafu};
use sqlx::{postgres::PgPoolOptions, FromRow, PgPool, Postgres, Transaction};
use tokio::{net::TcpListener, signal, task::JoinHandle};
use tokio_util::sync::CancellationToken;
use tracing::{error, info};
use tracing_subscriber::{fmt, layer::SubscriberExt as _, util::SubscriberInitExt as _};
use uuid::Uuid;

use threadplane_core::{
    load_threadplane_config, note_entity_ref, parse_entity_ref, relation_type, scope_summary,
    service_snapshot, task_entity_ref, AddLinkRequest, ApiEnvelope, ClaimTaskRequest,
    CreateNoteRequest, CreateXanaduLinkRequest, EntityRef, EventKind, EventRecord, GraphRelation,
    LinkRecord, NoteRecord, OfferTaskRequest, ServiceSnapshot, TaskClaimRecord, TaskContext,
    TaskRecord, TaskSummary, ThreadplaneConfig, UpdateNoteRequest, UpdateTaskRequest, SERVICE_NAME,
    XANADU_RELATION,
};

const NOTE_SELECT: &str = "
    SELECT
        note_id,
        event_id,
        workspace,
        author,
        title,
        body,
        transclusion_id,
        created_at,
        COALESCE(updated_at, created_at) AS updated_at
    FROM notes
";

const TASK_SELECT: &str = "
    SELECT
        task_id,
        event_id,
        workspace,
        author,
        title,
        details,
        status,
        transclusion_id,
        created_at,
        COALESCE(updated_at, created_at) AS updated_at
    FROM tasks
";
const MINIMUM_LEASE_SECONDS: i64 = 30;

type ServerResult<T, E = ThreadplaneServerError> = CoreResult<T, E>;

#[tokio::main]
async fn main() -> ServerResult<()> {
    drop(dotenv());
    init_tracing();

    let config = AppConfig::from_env()?;
    let shutdown = ShutdownCoordinator::new();
    let runtime = ServerRuntime::bootstrap(config).await?;
    let run_result = runtime.run(shutdown.token()).await;
    shutdown.shutdown().await;
    run_result
}

#[derive(Clone, Constructor)]
struct AppState {
    dependencies: AppDependencies,
    lease_policy: LeasePolicy,
}

impl AppState {
    const fn default_lease_seconds(&self) -> i64 {
        self.lease_policy.default_lease_seconds()
    }

    fn graph(&self) -> &Graph {
        self.dependencies.graph()
    }

    const fn pool(&self) -> &PgPool {
        self.dependencies.pool()
    }

    async fn shutdown(&self) {
        self.dependencies.shutdown().await;
    }
}

#[derive(Clone, Constructor)]
struct AppDependencies {
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
struct LeasePolicy {
    default_lease_seconds: i64,
}

impl LeasePolicy {
    const fn default_lease_seconds(self) -> i64 {
        self.default_lease_seconds
    }
}

struct ServerRuntime {
    bind_addr: SocketAddr,
    listener: TcpListener,
    state: AppState,
}

impl ServerRuntime {
    async fn bootstrap(config: AppConfig) -> ServerResult<Self> {
        let dependencies = connect_dependencies(&config).await?;
        let listener = bind_listener(config.bind_addr).await?;
        let lease_policy = LeasePolicy::new(config.default_lease_seconds);
        let state = AppState::new(dependencies, lease_policy);

        Ok(Self {
            bind_addr: config.bind_addr,
            listener,
            state,
        })
    }

    async fn run(self, shutdown_token: CancellationToken) -> ServerResult<()> {
        info!(service = SERVICE_NAME, bind_addr = %self.bind_addr, "server listening");

        let app = build_router(self.state.clone());
        let serve_result = serve(self.listener, app)
            .with_graceful_shutdown(wait_for_shutdown(shutdown_token))
            .await
            .context(Serve);
        self.state.shutdown().await;
        serve_result
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

struct AppConfig {
    bind_addr: SocketAddr,
    database_url: String,
    neo4j_uri: String,
    neo4j_user: String,
    neo4j_password: String,
    default_lease_seconds: i64,
}

impl AppConfig {
    fn from_env() -> ServerResult<Self> {
        let config = load_threadplane_config().context(LoadConfig)?;
        Self::from_threadplane_config(config)
    }

    fn from_threadplane_config(config: ThreadplaneConfig) -> ServerResult<Self> {
        let bind_addr = config.server.bind.parse().context(InvalidBindAddress {
            value: config.server.bind.clone(),
        })?;

        Ok(Self {
            bind_addr,
            database_url: required_config("server.database_url", config.server.database_url)?,
            neo4j_uri: required_config("server.neo4j_uri", config.server.neo4j_uri)?,
            neo4j_user: required_config("server.neo4j_user", config.server.neo4j_user)?,
            neo4j_password: required_config("server.neo4j_password", config.server.neo4j_password)?,
            default_lease_seconds: config.server.default_lease_seconds,
        })
    }
}

fn required_config(key: &str, value: Option<String>) -> ServerResult<String> {
    value
        .filter(|candidate| !candidate.is_empty())
        .ok_or_else(|| {
            MissingConfig {
                key: key.to_owned(),
            }
            .build()
        })
}

fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/", get(root))
        .route("/healthz", get(healthz))
        .route("/scope", get(scope))
        .route("/v1/notes", post(create_note))
        .route("/v1/notes/update", post(update_note))
        .route("/v1/notes/{note_id}", get(show_note))
        .route("/v1/tasks/offers", post(offer_task))
        .route("/v1/tasks/update", post(update_task))
        .route("/v1/tasks/claim", post(claim_task))
        .route("/v1/links", post(add_link))
        .route("/v1/links/xanadu", post(add_xanadu_link))
        .route("/v1/workspaces/{workspace}/events", get(list_events))
        .route(
            "/v1/workspaces/{workspace}/tasks/open",
            get(list_open_tasks),
        )
        .route("/v1/tasks/{task_id}/context", get(task_context))
        .with_state(state)
}

async fn bind_listener(bind_addr: SocketAddr) -> ServerResult<TcpListener> {
    TcpListener::bind(bind_addr)
        .await
        .context(BindListener { bind_addr })
}

async fn connect_dependencies(config: &AppConfig) -> ServerResult<AppDependencies> {
    let pool = connect_postgres(&config.database_url).await?;
    ensure_schema(&pool).await?;
    let graph = connect_neo4j(
        &config.neo4j_uri,
        &config.neo4j_user,
        &config.neo4j_password,
    )
    .await?;

    Ok(AppDependencies::new(graph, pool))
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

#[derive(Debug, Snafu)]
#[snafu(visibility(pub(crate)), context(suffix(false)))]
enum ThreadplaneServerError {
    #[snafu(display("failed to load threadplane config"))]
    LoadConfig {
        source: threadplane_core::ThreadplaneError,
        #[snafu(implicit)]
        location: Location,
    },

    #[snafu(display("invalid server.bind value: {value}"))]
    InvalidBindAddress {
        value: String,
        source: AddrParseError,
        #[snafu(implicit)]
        location: Location,
    },

    #[snafu(display("missing required configuration value {key}"))]
    MissingConfig {
        key: String,
        #[snafu(implicit)]
        location: Location,
    },

    #[snafu(display("failed to bind threadplane server to {bind_addr}"))]
    BindListener {
        bind_addr: SocketAddr,
        source: io::Error,
        #[snafu(implicit)]
        location: Location,
    },

    #[snafu(display("failed to connect to postgres at {database_url}"))]
    ConnectPostgres {
        database_url: String,
        source: sqlx::Error,
        #[snafu(implicit)]
        location: Location,
    },

    #[snafu(display("failed to connect to neo4j at {neo4j_uri}"))]
    ConnectNeo4j {
        neo4j_uri: String,
        source: neo4rs::Error,
        #[snafu(implicit)]
        location: Location,
    },

    #[snafu(display("failed to verify neo4j connectivity"))]
    VerifyNeo4j {
        source: neo4rs::Error,
        #[snafu(implicit)]
        location: Location,
    },

    #[snafu(display("threadplane server exited unexpectedly"))]
    Serve {
        source: io::Error,
        #[snafu(implicit)]
        location: Location,
    },

    #[snafu(display("database operation failed"))]
    Database {
        source: sqlx::Error,
        #[snafu(implicit)]
        location: Location,
    },

    #[snafu(display("graph operation failed"))]
    GraphOperation {
        source: neo4rs::Error,
        #[snafu(implicit)]
        location: Location,
    },

    #[snafu(display("json serialization failed"))]
    JsonSerialization {
        source: serde_json::Error,
        #[snafu(implicit)]
        location: Location,
    },

    #[snafu(display("graph row decoding failed"))]
    GraphDecode {
        source: neo4rs::DeError,
        #[snafu(implicit)]
        location: Location,
    },

    #[snafu(display("bad request: {msg}"))]
    BadRequest {
        msg: String,
        #[snafu(implicit)]
        location: Location,
    },

    #[snafu(display("conflict: {msg}"))]
    Conflict {
        msg: String,
        #[snafu(implicit)]
        location: Location,
    },

    #[snafu(display("not found: {msg}"))]
    NotFound {
        msg: String,
        #[snafu(implicit)]
        location: Location,
    },

    #[snafu(display("internal error: {msg}"))]
    Internal {
        msg: String,
        #[snafu(implicit)]
        location: Location,
    },
}

impl ThreadplaneServerError {
    fn bad_request(msg: impl Into<String>) -> Self {
        BadRequest { msg: msg.into() }.build()
    }

    fn conflict(msg: impl Into<String>) -> Self {
        Conflict { msg: msg.into() }.build()
    }

    fn not_found(msg: impl Into<String>) -> Self {
        NotFound { msg: msg.into() }.build()
    }

    fn internal(msg: impl Display) -> Self {
        Internal {
            msg: msg.to_string(),
        }
        .build()
    }

    const fn location(&self) -> &Location {
        match self {
            Self::LoadConfig { location, .. }
            | Self::InvalidBindAddress { location, .. }
            | Self::MissingConfig { location, .. }
            | Self::BindListener { location, .. }
            | Self::ConnectPostgres { location, .. }
            | Self::ConnectNeo4j { location, .. }
            | Self::VerifyNeo4j { location, .. }
            | Self::Serve { location, .. }
            | Self::Database { location, .. }
            | Self::GraphOperation { location, .. }
            | Self::JsonSerialization { location, .. }
            | Self::GraphDecode { location, .. }
            | Self::BadRequest { location, .. }
            | Self::Conflict { location, .. }
            | Self::NotFound { location, .. }
            | Self::Internal { location, .. } => location,
        }
    }

    const fn status_code(&self) -> StatusCode {
        match self {
            Self::BadRequest { .. } => StatusCode::BAD_REQUEST,
            Self::Conflict { .. } => StatusCode::CONFLICT,
            Self::NotFound { .. } => StatusCode::NOT_FOUND,
            Self::LoadConfig { .. }
            | Self::InvalidBindAddress { .. }
            | Self::MissingConfig { .. }
            | Self::BindListener { .. }
            | Self::ConnectPostgres { .. }
            | Self::ConnectNeo4j { .. }
            | Self::VerifyNeo4j { .. }
            | Self::Serve { .. }
            | Self::Database { .. }
            | Self::GraphOperation { .. }
            | Self::JsonSerialization { .. }
            | Self::GraphDecode { .. }
            | Self::Internal { .. } => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn response_message(&self) -> String {
        match self {
            Self::Database { source, .. } => source.to_string(),
            Self::GraphOperation { source, .. } => source.to_string(),
            Self::JsonSerialization { source, .. } => source.to_string(),
            Self::GraphDecode { source, .. } => source.to_string(),
            Self::LoadConfig { .. }
            | Self::InvalidBindAddress { .. }
            | Self::MissingConfig { .. }
            | Self::BindListener { .. }
            | Self::ConnectPostgres { .. }
            | Self::ConnectNeo4j { .. }
            | Self::VerifyNeo4j { .. }
            | Self::Serve { .. } => self.to_string(),
            Self::BadRequest { msg, .. }
            | Self::Conflict { msg, .. }
            | Self::NotFound { msg, .. }
            | Self::Internal { msg, .. } => msg.clone(),
        }
    }
}

impl IntoResponse for ThreadplaneServerError {
    fn into_response(self) -> Response {
        error!(error = %self, location = %self.location(), "request failed");
        (
            self.status_code(),
            Json(json!({
                "ok": false,
                "error": self.response_message(),
            })),
        )
            .into_response()
    }
}

impl From<sqlx::Error> for ThreadplaneServerError {
    fn from(value: sqlx::Error) -> Self {
        Database.into_error(value)
    }
}

impl From<neo4rs::Error> for ThreadplaneServerError {
    fn from(value: neo4rs::Error) -> Self {
        GraphOperation.into_error(value)
    }
}

impl From<serde_json::Error> for ThreadplaneServerError {
    fn from(value: serde_json::Error) -> Self {
        JsonSerialization.into_error(value)
    }
}

impl From<neo4rs::DeError> for ThreadplaneServerError {
    fn from(value: neo4rs::DeError) -> Self {
        GraphDecode.into_error(value)
    }
}

type AppResult<T> = ServerResult<Json<ApiEnvelope<T>>>;

#[derive(Debug, Deserialize)]
struct ListQuery {
    limit: Option<i64>,
}

async fn root() -> Json<ServiceSnapshot> {
    Json(service_snapshot())
}

async fn healthz() -> Json<Value> {
    Json(json!({
        "ok": true,
        "service": SERVICE_NAME,
    }))
}

async fn scope() -> Json<Value> {
    Json(scope_summary())
}

async fn create_note(
    State(state): State<AppState>,
    Json(request): Json<CreateNoteRequest>,
) -> AppResult<NoteRecord> {
    let mut tx = state.pool().begin().await?;
    let note_id = Uuid::new_v4();
    let created_at = Utc::now();
    let payload = serde_json::to_value(&request)?;
    let event_id = append_event(
        &mut tx,
        &request.workspace,
        &request.author,
        EventKind::NoteRecorded,
        &payload,
        created_at,
    )
    .await?;

    sqlx::query(
        "
        INSERT INTO notes (
            note_id,
            event_id,
            workspace,
            author,
            title,
            body,
            transclusion_id,
            created_at,
            updated_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, NULL, $7, $7)
        ",
    )
    .bind(note_id)
    .bind(event_id)
    .bind(&request.workspace)
    .bind(&request.author)
    .bind(&request.title)
    .bind(&request.body)
    .bind(created_at)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    let row = fetch_note_by_id(state.pool(), note_id).await?;
    let record = NoteRecord::from(row);
    project_note(state.graph(), &record)
        .await
        .map_err(|error| {
            error!(?error, note_id = %record.note_id, "failed to project note");
            ThreadplaneServerError::internal(error)
        })?;

    Ok(Json(ApiEnvelope {
        ok: true,
        data: record,
    }))
}

async fn show_note(
    State(state): State<AppState>,
    Path(note_id): Path<Uuid>,
) -> AppResult<NoteRecord> {
    let row = fetch_note_by_id(state.pool(), note_id).await?;
    Ok(Json(ApiEnvelope {
        ok: true,
        data: NoteRecord::from(row),
    }))
}

async fn update_note(
    State(state): State<AppState>,
    Json(request): Json<UpdateNoteRequest>,
) -> AppResult<NoteRecord> {
    let mut tx = state.pool().begin().await?;
    let note = fetch_note_by_id_tx(&mut tx, request.note_id, &request.workspace).await?;
    let updated_at = Utc::now();
    let payload = serde_json::to_value(&request)?;
    append_event(
        &mut tx,
        &request.workspace,
        &request.actor,
        EventKind::NoteUpdated,
        &payload,
        updated_at,
    )
    .await?;

    let transclusion_id = if let Some(transclusion_id) = note.transclusion_id {
        update_transclusion_group(
            &mut tx,
            transclusion_id,
            &request.workspace,
            &request.actor,
            &request.title,
            &request.body,
            updated_at,
        )
        .await?;
        sync_transclusion_members(&mut tx, transclusion_id).await?;
        Some(transclusion_id)
    } else {
        sqlx::query(
            "
            UPDATE notes
            SET title = $2,
                body = $3,
                updated_at = $4
            WHERE note_id = $1
            ",
        )
        .bind(request.note_id)
        .bind(&request.title)
        .bind(&request.body)
        .bind(updated_at)
        .execute(&mut *tx)
        .await?;
        None
    };

    tx.commit().await?;

    if let Some(group_id) = transclusion_id {
        reproject_transclusion_group(&state, group_id, None)
            .await
            .map_err(ThreadplaneServerError::internal)?;
    } else {
        let row = fetch_note_by_id(state.pool(), request.note_id).await?;
        project_note(state.graph(), &NoteRecord::from(row))
            .await
            .map_err(ThreadplaneServerError::internal)?;
    }

    let row = fetch_note_by_id(state.pool(), request.note_id).await?;
    Ok(Json(ApiEnvelope {
        ok: true,
        data: NoteRecord::from(row),
    }))
}

async fn offer_task(
    State(state): State<AppState>,
    Json(request): Json<OfferTaskRequest>,
) -> AppResult<TaskRecord> {
    let mut tx = state.pool().begin().await?;
    let task_id = Uuid::new_v4();
    let created_at = Utc::now();
    let payload = serde_json::to_value(&request)?;
    let event_id = append_event(
        &mut tx,
        &request.workspace,
        &request.author,
        EventKind::TaskOffered,
        &payload,
        created_at,
    )
    .await?;

    sqlx::query(
        "
        INSERT INTO tasks (
            task_id,
            event_id,
            workspace,
            author,
            title,
            details,
            status,
            transclusion_id,
            created_at,
            updated_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, 'open', NULL, $7, $7)
        ",
    )
    .bind(task_id)
    .bind(event_id)
    .bind(&request.workspace)
    .bind(&request.author)
    .bind(&request.title)
    .bind(&request.details)
    .bind(created_at)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    let row = fetch_task_by_id(state.pool(), task_id).await?;
    let record = TaskRecord::from(row);
    project_task(state.graph(), &record)
        .await
        .map_err(|error| {
            error!(?error, task_id = %record.task_id, "failed to project task");
            ThreadplaneServerError::internal(error)
        })?;

    Ok(Json(ApiEnvelope {
        ok: true,
        data: record,
    }))
}

async fn update_task(
    State(state): State<AppState>,
    Json(request): Json<UpdateTaskRequest>,
) -> AppResult<TaskRecord> {
    let mut tx = state.pool().begin().await?;
    let task = fetch_task_by_id_tx(&mut tx, request.task_id, &request.workspace).await?;
    let updated_at = Utc::now();
    let payload = serde_json::to_value(&request)?;
    append_event(
        &mut tx,
        &request.workspace,
        &request.actor,
        EventKind::TaskUpdated,
        &payload,
        updated_at,
    )
    .await?;

    let transclusion_id = if let Some(transclusion_id) = task.transclusion_id {
        update_transclusion_group(
            &mut tx,
            transclusion_id,
            &request.workspace,
            &request.actor,
            &request.title,
            &request.details,
            updated_at,
        )
        .await?;
        sync_transclusion_members(&mut tx, transclusion_id).await?;
        Some(transclusion_id)
    } else {
        sqlx::query(
            "
            UPDATE tasks
            SET title = $2,
                details = $3,
                updated_at = $4
            WHERE task_id = $1
            ",
        )
        .bind(request.task_id)
        .bind(&request.title)
        .bind(&request.details)
        .bind(updated_at)
        .execute(&mut *tx)
        .await?;
        None
    };

    tx.commit().await?;

    if let Some(group_id) = transclusion_id {
        reproject_transclusion_group(&state, group_id, None)
            .await
            .map_err(ThreadplaneServerError::internal)?;
    } else {
        let row = fetch_task_by_id(state.pool(), request.task_id).await?;
        project_task(state.graph(), &TaskRecord::from(row))
            .await
            .map_err(ThreadplaneServerError::internal)?;
    }

    let row = fetch_task_by_id(state.pool(), request.task_id).await?;
    Ok(Json(ApiEnvelope {
        ok: true,
        data: TaskRecord::from(row),
    }))
}

async fn claim_task(
    State(state): State<AppState>,
    Json(request): Json<ClaimTaskRequest>,
) -> AppResult<TaskClaimRecord> {
    let lease_seconds =
        normalized_lease_seconds(request.lease_seconds, state.default_lease_seconds());
    let mut tx = state.pool().begin().await?;

    let task = fetch_task_by_id_tx(&mut tx, request.task_id, &request.workspace).await?;

    let active_claim: Option<ClaimRow> = sqlx::query_as(
        "
        SELECT claim_id, task_id, workspace, actor, event_id, claimed_at, expires_at
        FROM task_claims
        WHERE task_id = $1
          AND released_at IS NULL
          AND expires_at > now()
        ORDER BY claimed_at DESC
        LIMIT 1
        ",
    )
    .bind(request.task_id)
    .fetch_optional(&mut *tx)
    .await?;

    if let Some(claim) = active_claim {
        return Err(ThreadplaneServerError::conflict(format!(
            "task already claimed by {} until {}",
            claim.actor,
            claim.expires_at.to_rfc3339()
        )));
    }

    let claimed_at = Utc::now();
    let expires_at = calculate_claim_expiry(claimed_at, lease_seconds)
        .ok_or_else(|| ThreadplaneServerError::bad_request("lease expiration overflow"))?;
    let payload = serde_json::to_value(&request)?;
    let event_id = append_event(
        &mut tx,
        &request.workspace,
        &request.actor,
        EventKind::TaskClaimed,
        &payload,
        claimed_at,
    )
    .await?;

    let claim_id = Uuid::new_v4();
    sqlx::query(
        "
        INSERT INTO task_claims (claim_id, task_id, workspace, actor, event_id, claimed_at, expires_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        ",
    )
    .bind(claim_id)
    .bind(request.task_id)
    .bind(&request.workspace)
    .bind(&request.actor)
    .bind(event_id)
    .bind(claimed_at)
    .bind(expires_at)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        "
        UPDATE tasks
        SET updated_at = $2
        WHERE task_id = $1
        ",
    )
    .bind(request.task_id)
    .bind(claimed_at)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    let record = TaskClaimRecord {
        claim_id,
        task_id: request.task_id,
        workspace: request.workspace,
        actor: request.actor,
        event_id,
        claimed_at: claimed_at.to_rfc3339(),
        expires_at: expires_at.to_rfc3339(),
    };

    project_claim(state.graph(), &task, &record)
        .await
        .map_err(|error| {
            error!(?error, task_id = %record.task_id, "failed to project claim");
            ThreadplaneServerError::internal(error)
        })?;

    Ok(Json(ApiEnvelope {
        ok: true,
        data: record,
    }))
}

async fn add_link(
    State(state): State<AppState>,
    Json(request): Json<AddLinkRequest>,
) -> AppResult<LinkRecord> {
    let mut tx = state.pool().begin().await?;
    let created_at = Utc::now();
    let payload = serde_json::to_value(&request)?;
    let event_id = append_event(
        &mut tx,
        &request.workspace,
        &request.actor,
        EventKind::LinkDeclared,
        &payload,
        created_at,
    )
    .await?;

    let link_id = Uuid::new_v4();
    sqlx::query(
        "
        INSERT INTO links (
            link_id,
            event_id,
            workspace,
            actor,
            from_entity_ref,
            to_entity_ref,
            relation,
            is_xanadu,
            transclusion_id,
            created_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, false, NULL, $8)
        ",
    )
    .bind(link_id)
    .bind(event_id)
    .bind(&request.workspace)
    .bind(&request.actor)
    .bind(&request.from)
    .bind(&request.to)
    .bind(&request.relation)
    .bind(created_at)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    let record = LinkRecord {
        link_id,
        event_id,
        workspace: request.workspace,
        actor: request.actor,
        from: request.from,
        to: request.to,
        relation: request.relation,
        is_xanadu: false,
        transclusion_id: None,
        created_at: created_at.to_rfc3339(),
    };

    project_link(state.graph(), &record)
        .await
        .map_err(|error| {
            error!(?error, link_id = %record.link_id, "failed to project link");
            ThreadplaneServerError::internal(error)
        })?;

    Ok(Json(ApiEnvelope {
        ok: true,
        data: record,
    }))
}

async fn add_xanadu_link(
    State(state): State<AppState>,
    Json(request): Json<CreateXanaduLinkRequest>,
) -> AppResult<LinkRecord> {
    let mut tx = state.pool().begin().await?;
    let created_at = Utc::now();
    let xanadu_group = prepare_xanadu_group(&mut tx, &request, created_at).await?;

    let payload = json!({
        "workspace": request.workspace,
        "actor": request.actor,
        "from": request.from,
        "to": request.to,
        "transclusion_id": xanadu_group.canonical_group_id,
    });
    let event_id = append_event(
        &mut tx,
        &request.workspace,
        &request.actor,
        EventKind::XanaduLinked,
        &payload,
        created_at,
    )
    .await?;

    let link_id = Uuid::new_v4();
    sqlx::query(
        "
        INSERT INTO links (
            link_id,
            event_id,
            workspace,
            actor,
            from_entity_ref,
            to_entity_ref,
            relation,
            is_xanadu,
            transclusion_id,
            created_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, true, $8, $9)
        ",
    )
    .bind(link_id)
    .bind(event_id)
    .bind(&request.workspace)
    .bind(&request.actor)
    .bind(&request.from)
    .bind(&request.to)
    .bind(XANADU_RELATION)
    .bind(xanadu_group.canonical_group_id)
    .bind(created_at)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    reproject_transclusion_group(
        &state,
        xanadu_group.canonical_group_id,
        xanadu_group.merged_group_id,
    )
    .await
    .map_err(ThreadplaneServerError::internal)?;

    Ok(Json(ApiEnvelope {
        ok: true,
        data: LinkRecord {
            link_id,
            event_id,
            workspace: request.workspace,
            actor: request.actor,
            from: request.from,
            to: request.to,
            relation: XANADU_RELATION.to_owned(),
            is_xanadu: true,
            transclusion_id: Some(xanadu_group.canonical_group_id),
            created_at: created_at.to_rfc3339(),
        },
    }))
}

struct XanaduGroup {
    canonical_group_id: Uuid,
    merged_group_id: Option<Uuid>,
}

async fn prepare_xanadu_group(
    tx: &mut Transaction<'_, Postgres>,
    request: &CreateXanaduLinkRequest,
    created_at: DateTime<Utc>,
) -> ServerResult<XanaduGroup> {
    let from = fetch_text_entity_by_ref_tx(tx, &request.workspace, &request.from).await?;
    let to = fetch_text_entity_by_ref_tx(tx, &request.workspace, &request.to).await?;
    let canonical_group_id = from
        .transclusion_id()
        .or_else(|| to.transclusion_id())
        .unwrap_or_else(Uuid::new_v4);
    let merged_group_id = match (from.transclusion_id(), to.transclusion_id()) {
        (Some(left), Some(right)) if left != right => Some(right),
        _ => None,
    };

    upsert_xanadu_group(
        tx,
        request,
        &from,
        canonical_group_id,
        merged_group_id,
        created_at,
    )
    .await?;
    set_entity_transclusion(tx, &from, canonical_group_id).await?;
    set_entity_transclusion(tx, &to, canonical_group_id).await?;
    sync_transclusion_members(tx, canonical_group_id).await?;

    Ok(XanaduGroup {
        canonical_group_id,
        merged_group_id,
    })
}

async fn upsert_xanadu_group(
    tx: &mut Transaction<'_, Postgres>,
    request: &CreateXanaduLinkRequest,
    from: &TextEntityRow,
    canonical_group_id: Uuid,
    merged_group_id: Option<Uuid>,
    created_at: DateTime<Utc>,
) -> ServerResult<()> {
    let source_title = from.title().to_owned();
    let source_content = from.content().to_owned();

    if group_exists(tx, canonical_group_id).await? {
        update_transclusion_group(
            tx,
            canonical_group_id,
            &request.workspace,
            &request.actor,
            &source_title,
            &source_content,
            created_at,
        )
        .await?;
    } else {
        insert_transclusion_group(
            tx,
            canonical_group_id,
            &request.workspace,
            &request.actor,
            &source_title,
            &source_content,
            created_at,
        )
        .await?;
    }

    if let Some(group_id) = merged_group_id {
        move_group_members(tx, group_id, canonical_group_id).await?;
        sqlx::query("DELETE FROM transclusion_groups WHERE transclusion_id = $1")
            .bind(group_id)
            .execute(&mut **tx)
            .await?;
    }

    Ok(())
}

async fn list_events(
    State(state): State<AppState>,
    Path(workspace): Path<String>,
    Query(query): Query<ListQuery>,
) -> AppResult<Vec<EventRecord>> {
    let limit = query.limit.unwrap_or(25).clamp(1, 200);
    let rows: Vec<EventRow> = sqlx::query_as(
        "
        SELECT event_id, workspace, actor, kind, payload, created_at
        FROM events
        WHERE workspace = $1
        ORDER BY created_at DESC
        LIMIT $2
        ",
    )
    .bind(&workspace)
    .bind(limit)
    .fetch_all(state.pool())
    .await?;

    let data = rows.into_iter().map(EventRecord::from).collect();
    Ok(Json(ApiEnvelope { ok: true, data }))
}

async fn list_open_tasks(
    State(state): State<AppState>,
    Path(workspace): Path<String>,
) -> AppResult<Vec<TaskSummary>> {
    let query = format!(
        "
        {TASK_SELECT}
        WHERE workspace = $1
          AND status = 'open'
          AND NOT EXISTS (
            SELECT 1
            FROM task_claims c
            WHERE c.task_id = tasks.task_id
              AND c.released_at IS NULL
              AND c.expires_at > now()
          )
        ORDER BY created_at DESC
        "
    );
    let rows: Vec<TaskRow> = sqlx::query_as(&query)
        .bind(&workspace)
        .fetch_all(state.pool())
        .await?;

    let data = rows.into_iter().map(TaskSummary::from).collect();
    Ok(Json(ApiEnvelope { ok: true, data }))
}

async fn task_context(
    State(state): State<AppState>,
    Path(task_id): Path<Uuid>,
) -> AppResult<TaskContext> {
    let task = fetch_task_by_id(state.pool(), task_id).await?;
    let active_claim: Option<ClaimRow> = sqlx::query_as(
        "
        SELECT claim_id, task_id, workspace, actor, event_id, claimed_at, expires_at
        FROM task_claims
        WHERE task_id = $1
          AND released_at IS NULL
          AND expires_at > now()
        ORDER BY claimed_at DESC
        LIMIT 1
        ",
    )
    .bind(task_id)
    .fetch_optional(state.pool())
    .await?;

    let relations = fetch_task_relations(state.graph(), task_id)
        .await
        .map_err(ThreadplaneServerError::internal)?;

    let data = TaskContext {
        task: task.into(),
        active_claim: active_claim.map(TaskClaimRecord::from),
        relations,
    };

    Ok(Json(ApiEnvelope { ok: true, data }))
}

async fn fetch_task_relations(graph: &Graph, task_id: Uuid) -> ServerResult<Vec<GraphRelation>> {
    let task_ref = task_entity_ref(task_id);
    let mut result = graph
        .execute(
            query(
                "
                MATCH (task:Entity {entity_ref: $task_ref})
                OPTIONAL MATCH (task)-[rel]-(other:Entity)
                RETURN
                  type(rel) AS relation,
                  CASE
                    WHEN rel IS NULL THEN NULL
                    WHEN startNode(rel).entity_ref = $task_ref THEN 'outgoing'
                    ELSE 'incoming'
                  END AS direction,
                  other.entity_ref AS entity_ref,
                  coalesce(other.kind, 'unknown') AS entity_kind,
                  other.title AS title,
                  coalesce(other.body, other.details) AS body,
                  NULLIF(other.transclusion_id, '') AS transclusion_id
                ORDER BY relation, entity_ref
                ",
            )
            .param("task_ref", task_ref),
        )
        .await?;

    let mut relations = Vec::new();
    loop {
        let maybe_row = result.next().await?;
        let Some(row) = maybe_row else {
            break;
        };

        let relation_opt: Option<String> = row.get("relation")?;
        let entity_ref_opt: Option<String> = row.get("entity_ref")?;
        let entity_kind_opt: Option<String> = row.get("entity_kind")?;
        let direction_opt: Option<String> = row.get("direction")?;
        let title: Option<String> = row.get("title")?;
        let body: Option<String> = row.get("body")?;
        let transclusion_id: Option<String> = row.get("transclusion_id")?;

        if let (Some(relation), Some(entity_ref), Some(entity_kind), Some(direction)) =
            (relation_opt, entity_ref_opt, entity_kind_opt, direction_opt)
        {
            relations.push(GraphRelation {
                relation,
                direction,
                entity_ref,
                entity_kind,
                title,
                body,
                transclusion_id: transclusion_id.and_then(|raw| Uuid::parse_str(&raw).ok()),
            });
        }
    }

    Ok(relations)
}

async fn project_note(graph: &Graph, note: &NoteRecord) -> ServerResult<()> {
    graph
        .run(
            query(
                "
                MERGE (workspace:Workspace {name: $workspace})
                MERGE (actor:Actor {name: $actor})
                MERGE (event:Event {event_id: $event_id})
                MERGE (note:Entity:Note {entity_ref: $entity_ref})
                SET note.kind = 'note',
                    note.note_id = $note_id,
                    note.workspace = $workspace,
                    note.title = $title,
                    note.body = $body,
                    note.transclusion_id = $transclusion_id,
                    note.created_at = $created_at,
                    note.updated_at = $updated_at
                MERGE (actor)-[:AUTHORED]->(note)
                MERGE (note)-[:RECORDED_IN]->(workspace)
                MERGE (note)-[:FROM_EVENT]->(event)
                MERGE (actor)-[:EMITTED]->(event)
                ",
            )
            .param("workspace", note.workspace.clone())
            .param("actor", note.author.clone())
            .param("event_id", note.event_id.to_string())
            .param("entity_ref", note.entity_ref.clone())
            .param("note_id", note.note_id.to_string())
            .param("title", note.title.clone())
            .param("body", note.body.clone())
            .param(
                "transclusion_id",
                note.transclusion_id
                    .map(|id| id.to_string())
                    .unwrap_or_default(),
            )
            .param("created_at", note.created_at.clone())
            .param("updated_at", note.updated_at.clone()),
        )
        .await?;
    Ok(())
}

async fn project_task(graph: &Graph, task: &TaskRecord) -> ServerResult<()> {
    graph
        .run(
            query(
                "
                MERGE (workspace:Workspace {name: $workspace})
                MERGE (actor:Actor {name: $actor})
                MERGE (event:Event {event_id: $event_id})
                MERGE (task:Entity:Task {entity_ref: $entity_ref})
                SET task.kind = 'task',
                    task.task_id = $task_id,
                    task.workspace = $workspace,
                    task.title = $title,
                    task.details = $details,
                    task.status = $status,
                    task.transclusion_id = $transclusion_id,
                    task.created_at = $created_at,
                    task.updated_at = $updated_at
                MERGE (actor)-[:AUTHORED]->(task)
                MERGE (task)-[:RECORDED_IN]->(workspace)
                MERGE (task)-[:FROM_EVENT]->(event)
                MERGE (actor)-[:EMITTED]->(event)
                ",
            )
            .param("workspace", task.workspace.clone())
            .param("actor", task.author.clone())
            .param("event_id", task.event_id.to_string())
            .param("entity_ref", task.entity_ref.clone())
            .param("task_id", task.task_id.to_string())
            .param("title", task.title.clone())
            .param("details", task.details.clone())
            .param("status", task.status.clone())
            .param(
                "transclusion_id",
                task.transclusion_id
                    .map(|id| id.to_string())
                    .unwrap_or_default(),
            )
            .param("created_at", task.created_at.clone())
            .param("updated_at", task.updated_at.clone()),
        )
        .await?;
    Ok(())
}

async fn project_claim(graph: &Graph, task: &TaskRow, claim: &TaskClaimRecord) -> ServerResult<()> {
    graph
        .run(
            query(
                "
                MERGE (actor:Actor {name: $actor})
                MERGE (task:Entity:Task {entity_ref: $task_ref})
                MERGE (event:Event {event_id: $event_id})
                MERGE (claim:Claim {claim_id: $claim_id})
                SET claim.workspace = $workspace,
                    claim.claimed_at = $claimed_at,
                    claim.expires_at = $expires_at
                MERGE (claim)-[:FOR_TASK]->(task)
                MERGE (claim)-[:HELD_BY]->(actor)
                MERGE (claim)-[:FROM_EVENT]->(event)
                SET task.status = $task_status
                ",
            )
            .param("actor", claim.actor.clone())
            .param("task_ref", task_entity_ref(task.task_id))
            .param("event_id", claim.event_id.to_string())
            .param("claim_id", claim.claim_id.to_string())
            .param("workspace", claim.workspace.clone())
            .param("claimed_at", claim.claimed_at.clone())
            .param("expires_at", claim.expires_at.clone())
            .param("task_status", task.status.clone()),
        )
        .await?;
    Ok(())
}

async fn project_link(graph: &Graph, link: &LinkRecord) -> ServerResult<()> {
    let relation = relation_type(&link.relation);
    let cypher = format!(
        "
        MERGE (event:Event {{event_id: $event_id}})
        MERGE (from:Entity {{entity_ref: $from}})
        ON CREATE SET from.kind = 'unknown'
        MERGE (to:Entity {{entity_ref: $to}})
        ON CREATE SET to.kind = 'unknown'
        MERGE (from)-[rel:{relation}]->(to)
        SET rel.workspace = $workspace,
            rel.actor = $actor,
            rel.created_at = $created_at,
            rel.event_id = $event_id,
            rel.is_xanadu = $is_xanadu,
            rel.transclusion_id = $transclusion_id
        MERGE (from)-[:LINKED_BY_EVENT]->(event)
        MERGE (to)-[:LINKED_BY_EVENT]->(event)
        "
    );

    graph
        .run(
            query(&cypher)
                .param("event_id", link.event_id.to_string())
                .param("from", link.from.clone())
                .param("to", link.to.clone())
                .param("workspace", link.workspace.clone())
                .param("actor", link.actor.clone())
                .param("created_at", link.created_at.clone())
                .param("is_xanadu", link.is_xanadu)
                .param(
                    "transclusion_id",
                    link.transclusion_id
                        .map(|id| id.to_string())
                        .unwrap_or_default(),
                ),
        )
        .await?;
    Ok(())
}

async fn reproject_transclusion_group(
    state: &AppState,
    group_id: Uuid,
    merged_group_id: Option<Uuid>,
) -> ServerResult<()> {
    if let Some(old_group_id) = merged_group_id {
        state
            .graph()
            .run(
                query("MATCH ()-[rel:XANADU_LINK {transclusion_id: $group_id}]-() DELETE rel")
                    .param("group_id", old_group_id.to_string()),
            )
            .await?;
    }

    state
        .graph()
        .run(
            query("MATCH ()-[rel:XANADU_LINK {transclusion_id: $group_id}]-() DELETE rel")
                .param("group_id", group_id.to_string()),
        )
        .await?;

    let notes: Vec<NoteRow> = sqlx::query_as(&format!(
        "
        {NOTE_SELECT}
        WHERE transclusion_id = $1
        ORDER BY note_id
        "
    ))
    .bind(group_id)
    .fetch_all(state.pool())
    .await?;

    let tasks: Vec<TaskRow> = sqlx::query_as(&format!(
        "
        {TASK_SELECT}
        WHERE transclusion_id = $1
        ORDER BY task_id
        "
    ))
    .bind(group_id)
    .fetch_all(state.pool())
    .await?;

    let mut entity_refs = Vec::new();

    for note in notes {
        let record = NoteRecord::from(note);
        entity_refs.push(record.entity_ref.clone());
        project_note(state.graph(), &record).await?;
    }

    for task in tasks {
        let record = TaskRecord::from(task);
        entity_refs.push(record.entity_ref.clone());
        project_task(state.graph(), &record).await?;
    }

    entity_refs.sort();
    for (index, left) in entity_refs.iter().enumerate() {
        for right in entity_refs
            .iter()
            .skip(index.checked_add(1).unwrap_or(entity_refs.len()))
        {
            state
                .graph()
                .run(
                    query(
                        "
                        MATCH (from:Entity {entity_ref: $from}), (to:Entity {entity_ref: $to})
                        MERGE (from)-[rel:XANADU_LINK]->(to)
                        SET rel.transclusion_id = $group_id
                        ",
                    )
                    .param("from", left.clone())
                    .param("to", right.clone())
                    .param("group_id", group_id.to_string()),
                )
                .await?;
        }
    }

    Ok(())
}

async fn ensure_schema(pool: &PgPool) -> ServerResult<()> {
    for statement in schema_statements() {
        sqlx::query(statement).execute(pool).await?;
    }

    Ok(())
}

const fn schema_statements() -> &'static [&'static str] {
    &[
        "
        CREATE TABLE IF NOT EXISTS events (
            event_id UUID PRIMARY KEY,
            workspace TEXT NOT NULL,
            actor TEXT NOT NULL,
            kind TEXT NOT NULL,
            payload JSONB NOT NULL,
            created_at TIMESTAMPTZ NOT NULL
        )
        ",
        "
        CREATE TABLE IF NOT EXISTS notes (
            note_id UUID PRIMARY KEY,
            event_id UUID NOT NULL REFERENCES events(event_id),
            workspace TEXT NOT NULL,
            author TEXT NOT NULL,
            title TEXT NOT NULL,
            body TEXT NOT NULL,
            created_at TIMESTAMPTZ NOT NULL
        )
        ",
        "
        CREATE TABLE IF NOT EXISTS tasks (
            task_id UUID PRIMARY KEY,
            event_id UUID NOT NULL REFERENCES events(event_id),
            workspace TEXT NOT NULL,
            author TEXT NOT NULL,
            title TEXT NOT NULL,
            details TEXT NOT NULL,
            status TEXT NOT NULL,
            created_at TIMESTAMPTZ NOT NULL,
            updated_at TIMESTAMPTZ NOT NULL
        )
        ",
        "
        CREATE TABLE IF NOT EXISTS task_claims (
            claim_id UUID PRIMARY KEY,
            task_id UUID NOT NULL REFERENCES tasks(task_id),
            workspace TEXT NOT NULL,
            actor TEXT NOT NULL,
            event_id UUID NOT NULL REFERENCES events(event_id),
            claimed_at TIMESTAMPTZ NOT NULL,
            expires_at TIMESTAMPTZ NOT NULL,
            released_at TIMESTAMPTZ NULL
        )
        ",
        "
        CREATE TABLE IF NOT EXISTS links (
            link_id UUID PRIMARY KEY,
            event_id UUID NOT NULL REFERENCES events(event_id),
            workspace TEXT NOT NULL,
            actor TEXT NOT NULL,
            from_entity_ref TEXT NOT NULL,
            to_entity_ref TEXT NOT NULL,
            relation TEXT NOT NULL,
            created_at TIMESTAMPTZ NOT NULL
        )
        ",
        "
        CREATE TABLE IF NOT EXISTS transclusion_groups (
            transclusion_id UUID PRIMARY KEY,
            workspace TEXT NOT NULL,
            created_by TEXT NOT NULL,
            title TEXT NOT NULL,
            content TEXT NOT NULL,
            created_at TIMESTAMPTZ NOT NULL,
            updated_at TIMESTAMPTZ NOT NULL
        )
        ",
        "ALTER TABLE notes ADD COLUMN IF NOT EXISTS transclusion_id UUID",
        "ALTER TABLE notes ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ",
        "ALTER TABLE tasks ADD COLUMN IF NOT EXISTS transclusion_id UUID",
        "ALTER TABLE links ADD COLUMN IF NOT EXISTS is_xanadu BOOLEAN NOT NULL DEFAULT false",
        "ALTER TABLE links ADD COLUMN IF NOT EXISTS transclusion_id UUID",
        "UPDATE notes SET updated_at = created_at WHERE updated_at IS NULL",
        "
        CREATE INDEX IF NOT EXISTS idx_events_workspace_created_at
        ON events (workspace, created_at DESC)
        ",
        "
        CREATE INDEX IF NOT EXISTS idx_tasks_workspace_status_created_at
        ON tasks (workspace, status, created_at DESC)
        ",
        "
        CREATE INDEX IF NOT EXISTS idx_task_claims_task_id_expires_at
        ON task_claims (task_id, expires_at DESC)
        ",
        "
        CREATE INDEX IF NOT EXISTS idx_notes_transclusion_id
        ON notes (transclusion_id)
        ",
        "
        CREATE INDEX IF NOT EXISTS idx_tasks_transclusion_id
        ON tasks (transclusion_id)
        ",
    ]
}

async fn append_event(
    tx: &mut Transaction<'_, Postgres>,
    workspace: &str,
    actor: &str,
    kind: EventKind,
    payload: &Value,
    created_at: DateTime<Utc>,
) -> ServerResult<Uuid> {
    let event_id = Uuid::new_v4();
    sqlx::query(
        "
        INSERT INTO events (event_id, workspace, actor, kind, payload, created_at)
        VALUES ($1, $2, $3, $4, $5, $6)
        ",
    )
    .bind(event_id)
    .bind(workspace)
    .bind(actor)
    .bind(event_kind_name(kind))
    .bind(payload.clone())
    .bind(created_at)
    .execute(&mut **tx)
    .await?;
    Ok(event_id)
}

async fn fetch_note_by_id(pool: &PgPool, note_id: Uuid) -> ServerResult<NoteRow> {
    sqlx::query_as(&format!("{NOTE_SELECT} WHERE note_id = $1"))
        .bind(note_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| ThreadplaneServerError::not_found("note not found"))
}

async fn fetch_note_by_id_tx(
    tx: &mut Transaction<'_, Postgres>,
    note_id: Uuid,
    workspace: &str,
) -> ServerResult<NoteRow> {
    sqlx::query_as(&format!(
        "{NOTE_SELECT} WHERE note_id = $1 AND workspace = $2"
    ))
    .bind(note_id)
    .bind(workspace)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| ThreadplaneServerError::not_found("note not found"))
}

async fn fetch_task_by_id(pool: &PgPool, task_id: Uuid) -> ServerResult<TaskRow> {
    sqlx::query_as(&format!("{TASK_SELECT} WHERE task_id = $1"))
        .bind(task_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| ThreadplaneServerError::not_found("task not found"))
}

async fn fetch_task_by_id_tx(
    tx: &mut Transaction<'_, Postgres>,
    task_id: Uuid,
    workspace: &str,
) -> ServerResult<TaskRow> {
    sqlx::query_as(&format!(
        "{TASK_SELECT} WHERE task_id = $1 AND workspace = $2"
    ))
    .bind(task_id)
    .bind(workspace)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| ThreadplaneServerError::not_found("task not found"))
}

async fn fetch_text_entity_by_ref_tx(
    tx: &mut Transaction<'_, Postgres>,
    workspace: &str,
    entity_ref: &str,
) -> ServerResult<TextEntityRow> {
    match parse_entity_ref(entity_ref) {
        Some(EntityRef::Note(note_id)) => Ok(TextEntityRow::Note(
            fetch_note_by_id_tx(tx, note_id, workspace).await?,
        )),
        Some(EntityRef::Task(task_id)) => Ok(TextEntityRow::Task(
            fetch_task_by_id_tx(tx, task_id, workspace).await?,
        )),
        None => Err(ThreadplaneServerError::bad_request(format!(
            "unsupported entity ref {entity_ref}"
        ))),
    }
}

async fn group_exists(
    tx: &mut Transaction<'_, Postgres>,
    transclusion_id: Uuid,
) -> ServerResult<bool> {
    let exists: Option<(Uuid,)> = sqlx::query_as(
        "SELECT transclusion_id FROM transclusion_groups WHERE transclusion_id = $1",
    )
    .bind(transclusion_id)
    .fetch_optional(&mut **tx)
    .await?;
    Ok(exists.is_some())
}

async fn insert_transclusion_group(
    tx: &mut Transaction<'_, Postgres>,
    transclusion_id: Uuid,
    workspace: &str,
    actor: &str,
    title: &str,
    content: &str,
    now: DateTime<Utc>,
) -> ServerResult<()> {
    sqlx::query(
        "
        INSERT INTO transclusion_groups (
            transclusion_id,
            workspace,
            created_by,
            title,
            content,
            created_at,
            updated_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $6)
        ",
    )
    .bind(transclusion_id)
    .bind(workspace)
    .bind(actor)
    .bind(title)
    .bind(content)
    .bind(now)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn update_transclusion_group(
    tx: &mut Transaction<'_, Postgres>,
    transclusion_id: Uuid,
    workspace: &str,
    _actor: &str,
    title: &str,
    content: &str,
    now: DateTime<Utc>,
) -> ServerResult<()> {
    sqlx::query(
        "
        UPDATE transclusion_groups
        SET workspace = $2,
            title = $3,
            content = $4,
            updated_at = $5
        WHERE transclusion_id = $1
        ",
    )
    .bind(transclusion_id)
    .bind(workspace)
    .bind(title)
    .bind(content)
    .bind(now)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn move_group_members(
    tx: &mut Transaction<'_, Postgres>,
    from_group_id: Uuid,
    to_group_id: Uuid,
) -> ServerResult<()> {
    sqlx::query("UPDATE notes SET transclusion_id = $2 WHERE transclusion_id = $1")
        .bind(from_group_id)
        .bind(to_group_id)
        .execute(&mut **tx)
        .await?;
    sqlx::query("UPDATE tasks SET transclusion_id = $2 WHERE transclusion_id = $1")
        .bind(from_group_id)
        .bind(to_group_id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

async fn set_entity_transclusion(
    tx: &mut Transaction<'_, Postgres>,
    entity: &TextEntityRow,
    transclusion_id: Uuid,
) -> ServerResult<()> {
    match entity {
        TextEntityRow::Note(note) => {
            sqlx::query("UPDATE notes SET transclusion_id = $2 WHERE note_id = $1")
                .bind(note.note_id)
                .bind(transclusion_id)
                .execute(&mut **tx)
                .await?;
        }
        TextEntityRow::Task(task) => {
            sqlx::query("UPDATE tasks SET transclusion_id = $2 WHERE task_id = $1")
                .bind(task.task_id)
                .bind(transclusion_id)
                .execute(&mut **tx)
                .await?;
        }
    }
    Ok(())
}

async fn sync_transclusion_members(
    tx: &mut Transaction<'_, Postgres>,
    transclusion_id: Uuid,
) -> ServerResult<()> {
    let group: TransclusionGroupRow = sqlx::query_as(
        "
        SELECT transclusion_id, workspace, title, content, created_at, updated_at
        FROM transclusion_groups
        WHERE transclusion_id = $1
        ",
    )
    .bind(transclusion_id)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| ThreadplaneServerError::not_found("transclusion group not found"))?;

    sqlx::query(
        "
        UPDATE notes
        SET title = $2,
            body = $3,
            updated_at = $4
        WHERE transclusion_id = $1
        ",
    )
    .bind(transclusion_id)
    .bind(&group.title)
    .bind(&group.content)
    .bind(group.updated_at)
    .execute(&mut **tx)
    .await?;

    sqlx::query(
        "
        UPDATE tasks
        SET title = $2,
            details = $3,
            updated_at = $4
        WHERE transclusion_id = $1
        ",
    )
    .bind(transclusion_id)
    .bind(&group.title)
    .bind(&group.content)
    .bind(group.updated_at)
    .execute(&mut **tx)
    .await?;

    Ok(())
}

fn event_kind_name(kind: EventKind) -> String {
    kind.to_string()
}

fn parse_event_kind(value: &str) -> EventKind {
    EventKind::from_str(value).unwrap_or(EventKind::NoteRecorded)
}

#[inline]
#[must_use]
fn calculate_claim_expiry(claimed_at: DateTime<Utc>, lease_seconds: i64) -> Option<DateTime<Utc>> {
    claimed_at.checked_add_signed(Duration::seconds(lease_seconds))
}

#[inline]
#[must_use]
fn normalized_lease_seconds(
    requested_lease_seconds: Option<i64>,
    default_lease_seconds: i64,
) -> i64 {
    requested_lease_seconds
        .unwrap_or(default_lease_seconds)
        .max(MINIMUM_LEASE_SECONDS)
}

#[derive(Debug, FromRow)]
struct EventRow {
    event_id: Uuid,
    workspace: String,
    actor: String,
    kind: String,
    payload: Value,
    created_at: DateTime<Utc>,
}

#[derive(Debug, FromRow, Clone)]
struct NoteRow {
    note_id: Uuid,
    event_id: Uuid,
    workspace: String,
    author: String,
    title: String,
    body: String,
    transclusion_id: Option<Uuid>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, FromRow, Clone)]
struct TaskRow {
    task_id: Uuid,
    event_id: Uuid,
    workspace: String,
    author: String,
    title: String,
    details: String,
    status: String,
    transclusion_id: Option<Uuid>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, FromRow)]
struct ClaimRow {
    claim_id: Uuid,
    task_id: Uuid,
    workspace: String,
    actor: String,
    event_id: Uuid,
    claimed_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
}

#[derive(Debug, FromRow)]
struct TransclusionGroupRow {
    title: String,
    content: String,
    updated_at: DateTime<Utc>,
}

enum TextEntityRow {
    Note(NoteRow),
    Task(TaskRow),
}

impl TextEntityRow {
    const fn transclusion_id(&self) -> Option<Uuid> {
        match self {
            Self::Note(note) => note.transclusion_id,
            Self::Task(task) => task.transclusion_id,
        }
    }

    fn title(&self) -> &str {
        match self {
            Self::Note(note) => &note.title,
            Self::Task(task) => &task.title,
        }
    }

    fn content(&self) -> &str {
        match self {
            Self::Note(note) => &note.body,
            Self::Task(task) => &task.details,
        }
    }
}

impl From<EventRow> for EventRecord {
    #[inline]
    fn from(value: EventRow) -> Self {
        Self {
            event_id: value.event_id,
            workspace: value.workspace,
            actor: value.actor,
            kind: parse_event_kind(&value.kind),
            payload: value.payload,
            created_at: value.created_at.to_rfc3339(),
        }
    }
}

impl From<NoteRow> for NoteRecord {
    #[inline]
    fn from(value: NoteRow) -> Self {
        Self {
            note_id: value.note_id,
            entity_ref: note_entity_ref(value.note_id),
            event_id: value.event_id,
            workspace: value.workspace,
            author: value.author,
            title: value.title,
            body: value.body,
            transclusion_id: value.transclusion_id,
            created_at: value.created_at.to_rfc3339(),
            updated_at: value.updated_at.to_rfc3339(),
        }
    }
}

impl From<TaskRow> for TaskRecord {
    #[inline]
    fn from(value: TaskRow) -> Self {
        Self {
            task_id: value.task_id,
            entity_ref: task_entity_ref(value.task_id),
            event_id: value.event_id,
            workspace: value.workspace,
            author: value.author,
            title: value.title,
            details: value.details,
            status: value.status,
            transclusion_id: value.transclusion_id,
            created_at: value.created_at.to_rfc3339(),
            updated_at: value.updated_at.to_rfc3339(),
        }
    }
}

impl From<TaskRow> for TaskSummary {
    #[inline]
    fn from(value: TaskRow) -> Self {
        Self {
            task_id: value.task_id,
            entity_ref: task_entity_ref(value.task_id),
            workspace: value.workspace,
            title: value.title,
            details: value.details,
            status: value.status,
            author: value.author,
            transclusion_id: value.transclusion_id,
            created_at: value.created_at.to_rfc3339(),
            updated_at: value.updated_at.to_rfc3339(),
        }
    }
}

impl From<ClaimRow> for TaskClaimRecord {
    #[inline]
    fn from(value: ClaimRow) -> Self {
        Self {
            claim_id: value.claim_id,
            task_id: value.task_id,
            workspace: value.workspace,
            actor: value.actor,
            event_id: value.event_id,
            claimed_at: value.claimed_at.to_rfc3339(),
            expires_at: value.expires_at.to_rfc3339(),
        }
    }
}

async fn wait_for_shutdown(shutdown_token: CancellationToken) {
    shutdown_token.cancelled().await;
}

async fn watch_for_shutdown_signal(shutdown_token: CancellationToken) {
    match signal::ctrl_c().await {
        Ok(()) => info!("shutdown signal received"),
        Err(error) => error!(
            ?error,
            "failed to listen for shutdown signal; cancelling runtime"
        ),
    }

    shutdown_token.cancel();
}

#[cfg(test)]
mod tests;
