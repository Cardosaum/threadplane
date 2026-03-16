use std::{net::SocketAddr, sync::Arc};

use anyhow::Context;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Duration, Utc};
use neo4rs::{query, Graph};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::{postgres::PgPoolOptions, FromRow, PgPool};
use tokio::{net::TcpListener, signal};
use tracing::{error, info};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use uuid::Uuid;

use threadplane_core::{
    relation_type, scope_summary, service_snapshot, task_entity_ref, AddLinkRequest, ApiEnvelope,
    ClaimTaskRequest, CreateNoteRequest, EventKind, EventRecord, GraphRelation, LinkRecord,
    NoteRecord, OfferTaskRequest, TaskClaimRecord, TaskContext, TaskRecord, TaskSummary,
    DEFAULT_BIND_ADDR, DEFAULT_LEASE_SECONDS, SERVICE_NAME,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = dotenvy::dotenv();

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "threadplane_server=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let config = AppConfig::from_env()?;
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&config.database_url)
        .await
        .with_context(|| format!("failed to connect to postgres at {}", config.database_url))?;
    ensure_schema(&pool).await?;

    let graph = Arc::new(
        Graph::new(
            &config.neo4j_uri,
            &config.neo4j_user,
            &config.neo4j_password,
        )
        .await
        .with_context(|| format!("failed to connect to neo4j at {}", config.neo4j_uri))?,
    );
    graph
        .run(query("RETURN 1"))
        .await
        .context("failed to verify neo4j connectivity")?;

    let state = AppState {
        pool,
        graph,
        default_lease_seconds: config.default_lease_seconds,
    };

    let listener = TcpListener::bind(config.bind_addr)
        .await
        .with_context(|| format!("failed to bind threadplane server to {}", config.bind_addr))?;

    let app = Router::new()
        .route("/", get(root))
        .route("/healthz", get(healthz))
        .route("/scope", get(scope))
        .route("/v1/notes", post(create_note))
        .route("/v1/tasks/offers", post(offer_task))
        .route("/v1/tasks/claim", post(claim_task))
        .route("/v1/links", post(add_link))
        .route("/v1/workspaces/{workspace}/events", get(list_events))
        .route(
            "/v1/workspaces/{workspace}/tasks/open",
            get(list_open_tasks),
        )
        .route("/v1/tasks/{task_id}/context", get(task_context))
        .with_state(state);

    info!(service = SERVICE_NAME, bind_addr = %config.bind_addr, "server listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("threadplane server exited unexpectedly")
}

#[derive(Clone)]
struct AppState {
    pool: PgPool,
    graph: Arc<Graph>,
    default_lease_seconds: i64,
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
    fn from_env() -> anyhow::Result<Self> {
        let bind_addr = std::env::var("THREADPLANE_BIND")
            .unwrap_or_else(|_| DEFAULT_BIND_ADDR.to_string())
            .parse()
            .unwrap_or_else(|_| {
                DEFAULT_BIND_ADDR
                    .parse()
                    .expect("default bind addr is valid")
            });

        Ok(Self {
            bind_addr,
            database_url: required_env("THREADPLANE_DATABASE_URL")?,
            neo4j_uri: required_env("THREADPLANE_NEO4J_URI")?,
            neo4j_user: required_env("THREADPLANE_NEO4J_USER")?,
            neo4j_password: required_env("THREADPLANE_NEO4J_PASSWORD")?,
            default_lease_seconds: std::env::var("THREADPLANE_DEFAULT_LEASE_SECONDS")
                .ok()
                .and_then(|raw| raw.parse().ok())
                .unwrap_or(DEFAULT_LEASE_SECONDS),
        })
    }
}

fn required_env(key: &str) -> anyhow::Result<String> {
    std::env::var(key).with_context(|| format!("missing required environment variable {key}"))
}

#[derive(Debug)]
struct AppError {
    status: StatusCode,
    message: String,
}

impl AppError {
    fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }

    fn internal(error: impl std::fmt::Display) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(json!({
                "ok": false,
                "error": self.message,
            })),
        )
            .into_response()
    }
}

impl From<sqlx::Error> for AppError {
    fn from(value: sqlx::Error) -> Self {
        Self::internal(value)
    }
}

impl From<anyhow::Error> for AppError {
    fn from(value: anyhow::Error) -> Self {
        Self::internal(value)
    }
}

type AppResult<T> = Result<Json<ApiEnvelope<T>>, AppError>;

#[derive(Debug, Deserialize)]
struct ListQuery {
    limit: Option<i64>,
}

async fn root() -> Json<threadplane_core::ServiceSnapshot> {
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
    let mut tx = state.pool.begin().await?;
    let event_id = Uuid::new_v4();
    let note_id = Uuid::new_v4();
    let created_at = Utc::now();
    let payload = serde_json::to_value(&request).map_err(AppError::internal)?;

    sqlx::query(
        r#"
        INSERT INTO events (event_id, workspace, actor, kind, payload, created_at)
        VALUES ($1, $2, $3, $4, $5, $6)
        "#,
    )
    .bind(event_id)
    .bind(&request.workspace)
    .bind(&request.author)
    .bind(event_kind_name(&EventKind::NoteRecorded))
    .bind(payload)
    .bind(created_at)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        r#"
        INSERT INTO notes (note_id, event_id, workspace, author, title, body, created_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        "#,
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

    let record = NoteRecord {
        note_id,
        entity_ref: threadplane_core::note_entity_ref(note_id),
        event_id,
        workspace: request.workspace,
        author: request.author,
        title: request.title,
        body: request.body,
        created_at: created_at.to_rfc3339(),
    };

    project_note(&state.graph, &record).await.map_err(|error| {
        error!(?error, note_id = %record.note_id, "failed to project note");
        AppError::internal(error)
    })?;

    Ok(Json(ApiEnvelope {
        ok: true,
        data: record,
    }))
}

async fn offer_task(
    State(state): State<AppState>,
    Json(request): Json<OfferTaskRequest>,
) -> AppResult<TaskRecord> {
    let mut tx = state.pool.begin().await?;
    let event_id = Uuid::new_v4();
    let task_id = Uuid::new_v4();
    let created_at = Utc::now();
    let payload = serde_json::to_value(&request).map_err(AppError::internal)?;

    sqlx::query(
        r#"
        INSERT INTO events (event_id, workspace, actor, kind, payload, created_at)
        VALUES ($1, $2, $3, $4, $5, $6)
        "#,
    )
    .bind(event_id)
    .bind(&request.workspace)
    .bind(&request.author)
    .bind(event_kind_name(&EventKind::TaskOffered))
    .bind(payload)
    .bind(created_at)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        r#"
        INSERT INTO tasks (task_id, event_id, workspace, author, title, details, status, created_at, updated_at)
        VALUES ($1, $2, $3, $4, $5, $6, 'open', $7, $7)
        "#,
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

    let record = TaskRecord {
        task_id,
        entity_ref: task_entity_ref(task_id),
        event_id,
        workspace: request.workspace,
        author: request.author,
        title: request.title,
        details: request.details,
        status: "open".to_string(),
        created_at: created_at.to_rfc3339(),
    };

    project_task(&state.graph, &record).await.map_err(|error| {
        error!(?error, task_id = %record.task_id, "failed to project task");
        AppError::internal(error)
    })?;

    Ok(Json(ApiEnvelope {
        ok: true,
        data: record,
    }))
}

async fn claim_task(
    State(state): State<AppState>,
    Json(request): Json<ClaimTaskRequest>,
) -> AppResult<TaskClaimRecord> {
    let lease_seconds = request
        .lease_seconds
        .unwrap_or(state.default_lease_seconds)
        .max(30);
    let mut tx = state.pool.begin().await?;

    let task: Option<TaskRow> = sqlx::query_as(
        r#"
        SELECT task_id, workspace, author, title, details, status, created_at
        FROM tasks
        WHERE task_id = $1 AND workspace = $2
        "#,
    )
    .bind(request.task_id)
    .bind(&request.workspace)
    .fetch_optional(&mut *tx)
    .await?;

    let task = task.ok_or_else(|| AppError::new(StatusCode::NOT_FOUND, "task not found"))?;

    let active_claim: Option<ClaimRow> = sqlx::query_as(
        r#"
        SELECT claim_id, task_id, workspace, actor, event_id, claimed_at, expires_at
        FROM task_claims
        WHERE task_id = $1
          AND released_at IS NULL
          AND expires_at > now()
        ORDER BY claimed_at DESC
        LIMIT 1
        "#,
    )
    .bind(request.task_id)
    .fetch_optional(&mut *tx)
    .await?;

    if let Some(claim) = active_claim {
        return Err(AppError::new(
            StatusCode::CONFLICT,
            format!(
                "task already claimed by {} until {}",
                claim.actor,
                claim.expires_at.to_rfc3339()
            ),
        ));
    }

    let event_id = Uuid::new_v4();
    let claim_id = Uuid::new_v4();
    let claimed_at = Utc::now();
    let expires_at = claimed_at + Duration::seconds(lease_seconds);
    let payload = serde_json::to_value(&request).map_err(AppError::internal)?;

    sqlx::query(
        r#"
        INSERT INTO events (event_id, workspace, actor, kind, payload, created_at)
        VALUES ($1, $2, $3, $4, $5, $6)
        "#,
    )
    .bind(event_id)
    .bind(&request.workspace)
    .bind(&request.actor)
    .bind(event_kind_name(&EventKind::TaskClaimed))
    .bind(payload)
    .bind(claimed_at)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        r#"
        INSERT INTO task_claims (claim_id, task_id, workspace, actor, event_id, claimed_at, expires_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        "#,
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
        r#"
        UPDATE tasks
        SET updated_at = $2
        WHERE task_id = $1
        "#,
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

    project_claim(&state.graph, &task, &record)
        .await
        .map_err(|error| {
            error!(?error, task_id = %record.task_id, "failed to project claim");
            AppError::internal(error)
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
    let mut tx = state.pool.begin().await?;
    let event_id = Uuid::new_v4();
    let link_id = Uuid::new_v4();
    let created_at = Utc::now();
    let payload = serde_json::to_value(&request).map_err(AppError::internal)?;

    sqlx::query(
        r#"
        INSERT INTO events (event_id, workspace, actor, kind, payload, created_at)
        VALUES ($1, $2, $3, $4, $5, $6)
        "#,
    )
    .bind(event_id)
    .bind(&request.workspace)
    .bind(&request.actor)
    .bind(event_kind_name(&EventKind::LinkDeclared))
    .bind(payload)
    .bind(created_at)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        r#"
        INSERT INTO links (link_id, event_id, workspace, actor, from_entity_ref, to_entity_ref, relation, created_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        "#,
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
        created_at: created_at.to_rfc3339(),
    };

    project_link(&state.graph, &record).await.map_err(|error| {
        error!(?error, link_id = %record.link_id, "failed to project link");
        AppError::internal(error)
    })?;

    Ok(Json(ApiEnvelope {
        ok: true,
        data: record,
    }))
}

async fn list_events(
    State(state): State<AppState>,
    Path(workspace): Path<String>,
    Query(query): Query<ListQuery>,
) -> AppResult<Vec<EventRecord>> {
    let limit = query.limit.unwrap_or(25).clamp(1, 200);
    let rows: Vec<EventRow> = sqlx::query_as(
        r#"
        SELECT event_id, workspace, actor, kind, payload, created_at
        FROM events
        WHERE workspace = $1
        ORDER BY created_at DESC
        LIMIT $2
        "#,
    )
    .bind(&workspace)
    .bind(limit)
    .fetch_all(&state.pool)
    .await?;

    let data = rows.into_iter().map(EventRecord::from).collect();
    Ok(Json(ApiEnvelope { ok: true, data }))
}

async fn list_open_tasks(
    State(state): State<AppState>,
    Path(workspace): Path<String>,
) -> AppResult<Vec<TaskSummary>> {
    let rows: Vec<TaskRow> = sqlx::query_as(
        r#"
        SELECT task_id, workspace, author, title, details, status, created_at
        FROM tasks t
        WHERE workspace = $1
          AND status = 'open'
          AND NOT EXISTS (
            SELECT 1
            FROM task_claims c
            WHERE c.task_id = t.task_id
              AND c.released_at IS NULL
              AND c.expires_at > now()
          )
        ORDER BY created_at DESC
        "#,
    )
    .bind(&workspace)
    .fetch_all(&state.pool)
    .await?;

    let data = rows.into_iter().map(TaskSummary::from).collect();
    Ok(Json(ApiEnvelope { ok: true, data }))
}

async fn task_context(
    State(state): State<AppState>,
    Path(task_id): Path<Uuid>,
) -> AppResult<TaskContext> {
    let task: Option<TaskRow> = sqlx::query_as(
        r#"
        SELECT task_id, workspace, author, title, details, status, created_at
        FROM tasks
        WHERE task_id = $1
        "#,
    )
    .bind(task_id)
    .fetch_optional(&state.pool)
    .await?;

    let task = task.ok_or_else(|| AppError::new(StatusCode::NOT_FOUND, "task not found"))?;
    let active_claim: Option<ClaimRow> = sqlx::query_as(
        r#"
        SELECT claim_id, task_id, workspace, actor, event_id, claimed_at, expires_at
        FROM task_claims
        WHERE task_id = $1
          AND released_at IS NULL
          AND expires_at > now()
        ORDER BY claimed_at DESC
        LIMIT 1
        "#,
    )
    .bind(task_id)
    .fetch_optional(&state.pool)
    .await?;

    let relations = fetch_task_relations(&state.graph, task_id)
        .await
        .map_err(AppError::internal)?;

    let data = TaskContext {
        task: task.into(),
        active_claim: active_claim.map(TaskClaimRecord::from),
        relations,
    };

    Ok(Json(ApiEnvelope { ok: true, data }))
}

async fn fetch_task_relations(graph: &Graph, task_id: Uuid) -> anyhow::Result<Vec<GraphRelation>> {
    let task_ref = task_entity_ref(task_id);
    let mut result = graph
        .execute(
            query(
                r#"
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
                  other.body AS body
                ORDER BY relation, entity_ref
                "#,
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

        let relation: Option<String> = row.get("relation")?;
        let entity_ref: Option<String> = row.get("entity_ref")?;
        let entity_kind: Option<String> = row.get("entity_kind")?;
        let direction: Option<String> = row.get("direction")?;
        let title: Option<String> = row.get("title")?;
        let body: Option<String> = row.get("body")?;

        if let (Some(relation), Some(entity_ref), Some(entity_kind), Some(direction)) =
            (relation, entity_ref, entity_kind, direction)
        {
            relations.push(GraphRelation {
                relation,
                direction,
                entity_ref,
                entity_kind,
                title,
                body,
            });
        }
    }

    Ok(relations)
}

async fn project_note(graph: &Graph, note: &NoteRecord) -> anyhow::Result<()> {
    graph
        .run(
            query(
                r#"
                MERGE (workspace:Workspace {name: $workspace})
                MERGE (actor:Actor {name: $actor})
                MERGE (event:Event {event_id: $event_id})
                MERGE (note:Entity:Note {entity_ref: $entity_ref})
                SET note.kind = 'note',
                    note.note_id = $note_id,
                    note.workspace = $workspace,
                    note.title = $title,
                    note.body = $body,
                    note.created_at = $created_at
                MERGE (actor)-[:AUTHORED]->(note)
                MERGE (note)-[:RECORDED_IN]->(workspace)
                MERGE (note)-[:FROM_EVENT]->(event)
                MERGE (actor)-[:EMITTED]->(event)
                "#,
            )
            .param("workspace", note.workspace.clone())
            .param("actor", note.author.clone())
            .param("event_id", note.event_id.to_string())
            .param("entity_ref", note.entity_ref.clone())
            .param("note_id", note.note_id.to_string())
            .param("title", note.title.clone())
            .param("body", note.body.clone())
            .param("created_at", note.created_at.clone()),
        )
        .await?;
    Ok(())
}

async fn project_task(graph: &Graph, task: &TaskRecord) -> anyhow::Result<()> {
    graph
        .run(
            query(
                r#"
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
                    task.created_at = $created_at
                MERGE (actor)-[:AUTHORED]->(task)
                MERGE (task)-[:RECORDED_IN]->(workspace)
                MERGE (task)-[:FROM_EVENT]->(event)
                MERGE (actor)-[:EMITTED]->(event)
                "#,
            )
            .param("workspace", task.workspace.clone())
            .param("actor", task.author.clone())
            .param("event_id", task.event_id.to_string())
            .param("entity_ref", task.entity_ref.clone())
            .param("task_id", task.task_id.to_string())
            .param("title", task.title.clone())
            .param("details", task.details.clone())
            .param("status", task.status.clone())
            .param("created_at", task.created_at.clone()),
        )
        .await?;
    Ok(())
}

async fn project_claim(
    graph: &Graph,
    task: &TaskRow,
    claim: &TaskClaimRecord,
) -> anyhow::Result<()> {
    graph
        .run(
            query(
                r#"
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
                "#,
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

async fn project_link(graph: &Graph, link: &LinkRecord) -> anyhow::Result<()> {
    let relation = relation_type(&link.relation);
    let cypher = format!(
        r#"
        MERGE (event:Event {{event_id: $event_id}})
        MERGE (from:Entity {{entity_ref: $from}})
        ON CREATE SET from.kind = 'unknown'
        MERGE (to:Entity {{entity_ref: $to}})
        ON CREATE SET to.kind = 'unknown'
        MERGE (from)-[rel:{relation}]->(to)
        SET rel.workspace = $workspace,
            rel.actor = $actor,
            rel.created_at = $created_at,
            rel.event_id = $event_id
        MERGE (from)-[:LINKED_BY_EVENT]->(event)
        MERGE (to)-[:LINKED_BY_EVENT]->(event)
        "#
    );

    graph
        .run(
            query(&cypher)
                .param("event_id", link.event_id.to_string())
                .param("from", link.from.clone())
                .param("to", link.to.clone())
                .param("workspace", link.workspace.clone())
                .param("actor", link.actor.clone())
                .param("created_at", link.created_at.clone()),
        )
        .await?;
    Ok(())
}

async fn ensure_schema(pool: &PgPool) -> anyhow::Result<()> {
    let statements = [
        r#"
        CREATE TABLE IF NOT EXISTS events (
            event_id UUID PRIMARY KEY,
            workspace TEXT NOT NULL,
            actor TEXT NOT NULL,
            kind TEXT NOT NULL,
            payload JSONB NOT NULL,
            created_at TIMESTAMPTZ NOT NULL
        )
        "#,
        r#"
        CREATE TABLE IF NOT EXISTS notes (
            note_id UUID PRIMARY KEY,
            event_id UUID NOT NULL REFERENCES events(event_id),
            workspace TEXT NOT NULL,
            author TEXT NOT NULL,
            title TEXT NOT NULL,
            body TEXT NOT NULL,
            created_at TIMESTAMPTZ NOT NULL
        )
        "#,
        r#"
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
        "#,
        r#"
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
        "#,
        r#"
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
        "#,
        r#"
        CREATE INDEX IF NOT EXISTS idx_events_workspace_created_at
        ON events (workspace, created_at DESC)
        "#,
        r#"
        CREATE INDEX IF NOT EXISTS idx_tasks_workspace_status_created_at
        ON tasks (workspace, status, created_at DESC)
        "#,
        r#"
        CREATE INDEX IF NOT EXISTS idx_task_claims_task_id_expires_at
        ON task_claims (task_id, expires_at DESC)
        "#,
    ];

    for statement in statements {
        sqlx::query(statement).execute(pool).await?;
    }

    Ok(())
}

fn event_kind_name(kind: &EventKind) -> String {
    serde_json::to_value(kind)
        .ok()
        .and_then(|value| value.as_str().map(ToOwned::to_owned))
        .unwrap_or_else(|| "unknown".to_string())
}

fn parse_event_kind(value: &str) -> EventKind {
    match value {
        "note_recorded" => EventKind::NoteRecorded,
        "link_declared" => EventKind::LinkDeclared,
        "task_offered" => EventKind::TaskOffered,
        "task_claimed" => EventKind::TaskClaimed,
        "task_released" => EventKind::TaskReleased,
        "fact_promoted" => EventKind::FactPromoted,
        _ => EventKind::NoteRecorded,
    }
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

#[derive(Debug, FromRow)]
struct TaskRow {
    task_id: Uuid,
    workspace: String,
    author: String,
    title: String,
    details: String,
    status: String,
    created_at: DateTime<Utc>,
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

impl From<EventRow> for EventRecord {
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

impl From<TaskRow> for TaskSummary {
    fn from(value: TaskRow) -> Self {
        Self {
            task_id: value.task_id,
            entity_ref: task_entity_ref(value.task_id),
            workspace: value.workspace,
            title: value.title,
            details: value.details,
            status: value.status,
            author: value.author,
            created_at: value.created_at.to_rfc3339(),
        }
    }
}

impl From<ClaimRow> for TaskClaimRecord {
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

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = signal::ctrl_c().await;
    };

    ctrl_c.await;
    info!("shutdown signal received");
}
