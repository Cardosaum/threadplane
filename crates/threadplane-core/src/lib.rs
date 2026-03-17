use std::{env, path::PathBuf};

use derive_more::Display;
use figment::{
    providers::{Env, Format as _, Serialized, Toml},
    Figment,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use snafu::{ResultExt as _, Snafu};
use strum::IntoEnumIterator as _;
use uuid::Uuid;

pub const SERVICE_NAME: &str = "threadplane";
pub const DEFAULT_BIND_ADDR: &str = "127.0.0.1:4000";
pub const DEFAULT_CONFIG_PATH: &str = "etc/config.toml";
pub const DEFAULT_SYSTEM_CONFIG_PATH: &str = "/etc/threadplane/config.toml";
pub const DEFAULT_SERVER_URL: &str = "http://127.0.0.1:4000";
pub const DEFAULT_LEASE_SECONDS: i64 = 300;
pub const XANADU_RELATION: &str = "xanadu_link";

#[derive(Debug, Snafu)]
#[snafu(visibility(pub), context(suffix(false)))]
pub enum ThreadplaneError {
    #[snafu(display("configuration load failed: {source}"))]
    ConfigLoad {
        #[snafu(source(from(figment::Error, Box::new)))]
        source: Box<figment::Error>,
        #[snafu(implicit)]
        location: snafu::Location,
    },
}

impl ThreadplaneError {
    #[inline]
    #[must_use]
    pub const fn location(&self) -> &snafu::Location {
        match self {
            Self::ConfigLoad { location, .. } => location,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliConfig {
    pub url: String,
}

impl Default for CliConfig {
    #[inline]
    fn default() -> Self {
        Self {
            url: DEFAULT_SERVER_URL.to_owned(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub bind: String,
    pub database_url: Option<String>,
    pub default_lease_seconds: i64,
    pub neo4j_password: Option<String>,
    pub neo4j_uri: Option<String>,
    pub neo4j_user: Option<String>,
}

impl Default for ServerConfig {
    #[inline]
    fn default() -> Self {
        Self {
            bind: DEFAULT_BIND_ADDR.to_owned(),
            database_url: None,
            default_lease_seconds: DEFAULT_LEASE_SECONDS,
            neo4j_password: None,
            neo4j_uri: None,
            neo4j_user: None,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ThreadplaneConfig {
    pub cli: CliConfig,
    pub server: ServerConfig,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    strum::Display,
    strum::EnumIter,
    strum::EnumString,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum EventKind {
    FactPromoted,
    LinkDeclared,
    NoteRecorded,
    NoteUpdated,
    TaskClaimed,
    TaskOffered,
    TaskReleased,
    TaskUpdated,
    XanaduLinked,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventEnvelope {
    pub actor: String,
    pub kind: EventKind,
    pub payload: Value,
    pub workspace: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceSnapshot {
    pub event_kinds: Vec<EventKind>,
    pub graph_projection: String,
    pub name: String,
    pub source_of_truth: String,
    pub summary: String,
    pub tuple_space: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateNoteRequest {
    pub author: String,
    pub body: String,
    pub title: String,
    pub workspace: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateNoteRequest {
    pub actor: String,
    pub body: String,
    pub note_id: Uuid,
    pub title: String,
    pub workspace: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OfferTaskRequest {
    pub author: String,
    pub details: String,
    pub title: String,
    pub workspace: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateTaskRequest {
    pub actor: String,
    pub details: String,
    pub task_id: Uuid,
    pub title: String,
    pub workspace: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaimTaskRequest {
    pub actor: String,
    pub lease_seconds: Option<i64>,
    pub task_id: Uuid,
    pub workspace: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddLinkRequest {
    pub actor: String,
    pub from: String,
    pub relation: String,
    pub to: String,
    pub workspace: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateXanaduLinkRequest {
    pub actor: String,
    pub from: String,
    pub to: String,
    pub workspace: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteRecord {
    pub author: String,
    pub body: String,
    pub created_at: String,
    pub entity_ref: String,
    pub event_id: Uuid,
    pub note_id: Uuid,
    pub title: String,
    pub transclusion_id: Option<Uuid>,
    pub updated_at: String,
    pub workspace: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRecord {
    pub author: String,
    pub created_at: String,
    pub details: String,
    pub entity_ref: String,
    pub event_id: Uuid,
    pub status: String,
    pub task_id: Uuid,
    pub title: String,
    pub transclusion_id: Option<Uuid>,
    pub updated_at: String,
    pub workspace: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskClaimRecord {
    pub actor: String,
    pub claim_id: Uuid,
    pub claimed_at: String,
    pub event_id: Uuid,
    pub expires_at: String,
    pub task_id: Uuid,
    pub workspace: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkRecord {
    pub actor: String,
    pub created_at: String,
    pub event_id: Uuid,
    pub from: String,
    pub is_xanadu: bool,
    pub link_id: Uuid,
    pub relation: String,
    pub to: String,
    pub transclusion_id: Option<Uuid>,
    pub workspace: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventRecord {
    pub actor: String,
    pub created_at: String,
    pub event_id: Uuid,
    pub kind: EventKind,
    pub payload: Value,
    pub workspace: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSummary {
    pub author: String,
    pub created_at: String,
    pub details: String,
    pub entity_ref: String,
    pub status: String,
    pub task_id: Uuid,
    pub title: String,
    pub transclusion_id: Option<Uuid>,
    pub updated_at: String,
    pub workspace: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphRelation {
    pub body: Option<String>,
    pub direction: String,
    pub entity_kind: String,
    pub entity_ref: String,
    pub relation: String,
    pub title: Option<String>,
    pub transclusion_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskContext {
    pub active_claim: Option<TaskClaimRecord>,
    pub relations: Vec<GraphRelation>,
    pub task: TaskSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiEnvelope<T> {
    pub data: T,
    pub ok: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Display)]
pub enum EntityRef {
    #[display("note:{_0}")]
    Note(Uuid),
    #[display("task:{_0}")]
    Task(Uuid),
}

#[inline]
#[must_use]
pub fn service_snapshot() -> ServiceSnapshot {
    ServiceSnapshot {
        event_kinds: EventKind::iter().collect(),
        graph_projection: "Neo4j projection for notes, dependencies, provenance, and traversal"
            .to_owned(),
        name: SERVICE_NAME.to_owned(),
        source_of_truth: "PostgreSQL append-only event log managed by threadplane-server"
            .to_owned(),
        summary: "Shared human/agent memory and coordination plane".to_owned(),
        tuple_space:
            "Service-managed tuple semantics with PostgreSQL persistence and lease-based claims"
                .to_owned(),
    }
}

#[inline]
#[must_use]
pub fn scope_summary() -> Value {
    json!({
        "name": SERVICE_NAME,
        "poc": {
            "goal": "Validate shared agent collaboration over an internet-reachable event log and graph projection",
            "service_boundary": "All writes pass through threadplane-server",
            "authoritative_log": "postgresql",
            "graph_projection": "neo4j",
            "tuple_coordination": "implemented in the service with postgres-backed leases",
            "future_influence": "VarveDB remains a candidate for local replicas and offline-first ingest buffers",
            "xanadu_links": "textual note/task entities can join a shared transclusion group so edits on one side propagate to the others"
        }
    })
}

#[inline]
#[must_use]
pub fn task_entity_ref(task_id: Uuid) -> String {
    EntityRef::Task(task_id).to_string()
}

#[inline]
#[must_use]
pub fn note_entity_ref(note_id: Uuid) -> String {
    EntityRef::Note(note_id).to_string()
}

#[inline]
#[must_use]
pub fn parse_entity_ref(input: &str) -> Option<EntityRef> {
    let (kind, raw_id) = input.split_once(':')?;
    let id = Uuid::parse_str(raw_id).ok()?;
    match kind {
        "note" => Some(EntityRef::Note(id)),
        "task" => Some(EntityRef::Task(id)),
        _ => None,
    }
}

#[inline]
#[must_use]
pub fn relation_type(input: &str) -> String {
    let mut last_was_underscore = false;
    let mut relation = String::new();

    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            relation.push(ch.to_ascii_uppercase());
            last_was_underscore = false;
            continue;
        }

        if !last_was_underscore {
            relation.push('_');
            last_was_underscore = true;
        }
    }

    relation.trim_matches('_').to_owned()
}

#[inline]
#[must_use]
pub fn default_config_path() -> PathBuf {
    PathBuf::from(DEFAULT_CONFIG_PATH)
}

#[inline]
#[must_use]
pub fn default_system_config_path() -> PathBuf {
    PathBuf::from(DEFAULT_SYSTEM_CONFIG_PATH)
}

#[inline]
/// Loads layered runtime configuration from defaults, optional TOML, and environment overrides.
///
/// # Errors
///
/// Returns an error when the optional config file cannot be parsed or when
/// the gathered values cannot be deserialized into [`ThreadplaneConfig`].
pub fn load_threadplane_config() -> Result<ThreadplaneConfig, ThreadplaneError> {
    let figment = config_path_from_env()
        .or_else(local_config_path_if_present)
        .or_else(system_config_path_if_present)
        .map_or_else(
            || {
                Figment::from(Serialized::defaults(ThreadplaneConfig::default()))
                    .merge(Env::prefixed("THREADPLANE__").split("__"))
            },
            |config_path| {
                Figment::from(Serialized::defaults(ThreadplaneConfig::default()))
                    .merge(Toml::file(config_path))
                    .merge(Env::prefixed("THREADPLANE__").split("__"))
            },
        );

    figment.extract().context(ConfigLoad)
}

#[inline]
#[must_use]
fn config_path_from_env() -> Option<PathBuf> {
    env::var("THREADPLANE_CONFIG")
        .ok()
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

#[inline]
#[must_use]
fn local_config_path_if_present() -> Option<PathBuf> {
    let config_path = default_config_path();
    config_path.exists().then_some(config_path)
}

#[inline]
#[must_use]
fn system_config_path_if_present() -> Option<PathBuf> {
    let config_path = default_system_config_path();
    config_path.exists().then_some(config_path)
}

#[cfg(test)]
mod tests;
