use derive_more::Display;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use strum::IntoEnumIterator as _;
use uuid::Uuid;

use crate::config::SERVICE_NAME;

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
    EpicRecorded,
    FactPromoted,
    LinkDeclared,
    NoteRecorded,
    NoteUpdated,
    TaskClaimed,
    TaskCompleted,
    TaskDependencyDeclared,
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
pub struct BuildInfo {
    pub build_profile: String,
    pub git_commit: Option<String>,
    pub git_dirty: bool,
    pub service: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildFieldDifference {
    pub client: String,
    pub field: String,
    pub server: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildComparison {
    pub client: BuildInfo,
    pub differences: Vec<BuildFieldDifference>,
    pub matches: bool,
    pub server: BuildInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceSnapshot {
    pub build: BuildInfo,
    pub event_kinds: Vec<EventKind>,
    pub graph_projection: String,
    pub name: String,
    pub source_of_truth: String,
    pub summary: String,
    pub tuple_space: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectionStatus {
    pub caught_up: bool,
    pub last_event_created_at: Option<String>,
    pub last_event_id: Option<Uuid>,
    pub pending_events: i64,
    pub projected_events: i64,
    pub projection_name: String,
    pub total_events: i64,
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
    pub depends_on: Vec<Uuid>,
    pub details: String,
    pub epic_id: Option<Uuid>,
    #[serde(flatten)]
    pub metadata: TaskMetadata,
    pub title: String,
    pub workspace: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateTaskRequest {
    pub actor: String,
    pub details: String,
    pub epic_id: Option<Uuid>,
    #[serde(flatten)]
    pub metadata: TaskMetadata,
    pub task_id: Uuid,
    pub title: String,
    pub workspace: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateEpicRequest {
    pub author: String,
    pub body: String,
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
pub struct ReleaseTaskRequest {
    pub actor: String,
    pub task_id: Uuid,
    pub workspace: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompleteTaskRequest {
    pub actor: String,
    pub task_id: Uuid,
    pub workspace: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddTaskDependencyRequest {
    pub actor: String,
    pub depends_on_task_id: Uuid,
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
pub struct EpicRecord {
    pub author: String,
    pub body: String,
    pub created_at: String,
    pub entity_ref: String,
    pub epic_id: Uuid,
    pub event_id: Uuid,
    pub title: String,
    pub updated_at: String,
    pub workspace: String,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Default,
    Serialize,
    Deserialize,
    strum::Display,
    strum::EnumIter,
    strum::EnumString,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum TaskPriority {
    High,
    Low,
    #[default]
    Medium,
    Urgent,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskMetadata {
    pub labels: Vec<String>,
    pub owner: Option<String>,
    pub priority: TaskPriority,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRecord {
    pub author: String,
    pub created_at: String,
    pub details: String,
    pub entity_ref: String,
    pub epic_id: Option<Uuid>,
    pub event_id: Uuid,
    #[serde(flatten)]
    pub metadata: TaskMetadata,
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
    pub epic_id: Option<Uuid>,
    #[serde(flatten)]
    pub metadata: TaskMetadata,
    pub status: String,
    pub task_id: Uuid,
    pub title: String,
    pub transclusion_id: Option<Uuid>,
    pub updated_at: String,
    pub workspace: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskDependencySummary {
    pub depth: i32,
    pub entity_ref: String,
    pub status: String,
    pub task_id: Uuid,
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskListEntry {
    pub active_claim: Option<TaskClaimRecord>,
    pub dependencies: Vec<TaskDependencySummary>,
    pub dependents: Vec<TaskDependencySummary>,
    pub epic: Option<EpicRecord>,
    pub ready: bool,
    pub task: TaskSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskDag {
    pub dependencies: Vec<TaskDependencySummary>,
    pub dependents: Vec<TaskDependencySummary>,
    pub epic: Option<EpicRecord>,
    pub ready: bool,
    pub task: TaskSummary,
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
    pub dependencies: Vec<TaskDependencySummary>,
    pub dependents: Vec<TaskDependencySummary>,
    pub epic: Option<EpicRecord>,
    pub ready: bool,
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
    #[display("epic:{_0}")]
    Epic(Uuid),
    #[display("note:{_0}")]
    Note(Uuid),
    #[display("task:{_0}")]
    Task(Uuid),
}

#[inline]
#[must_use]
pub fn normalize_task_labels(labels: Vec<String>) -> Vec<String> {
    let mut normalized = labels
        .into_iter()
        .map(|label| label.trim().to_ascii_lowercase())
        .filter(|label| !label.is_empty())
        .collect::<Vec<_>>();
    normalized.sort_unstable();
    normalized.dedup();
    normalized
}

#[inline]
#[must_use]
pub fn normalize_task_owner(owner: Option<String>) -> Option<String> {
    let value = owner?;
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

#[inline]
#[must_use]
pub fn build_info(
    service: &str,
    version: &str,
    build_profile: &str,
    git_commit: Option<&str>,
    git_dirty: bool,
) -> BuildInfo {
    BuildInfo {
        build_profile: build_profile.to_owned(),
        git_commit: git_commit.map(ToOwned::to_owned),
        git_dirty,
        service: service.to_owned(),
        version: version.to_owned(),
    }
}

#[inline]
#[must_use]
pub fn service_snapshot(build: BuildInfo) -> ServiceSnapshot {
    ServiceSnapshot {
        build,
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
pub fn health_summary(build: &BuildInfo) -> Value {
    json!({
        "ok": true,
        "service": SERVICE_NAME,
        "build": build,
    })
}

#[inline]
#[must_use]
pub fn compare_build_info(client: &BuildInfo, server: &BuildInfo) -> BuildComparison {
    let mut differences = Vec::new();

    push_build_difference(
        &mut differences,
        "version",
        &client.version,
        &server.version,
    );
    push_build_difference(
        &mut differences,
        "build_profile",
        &client.build_profile,
        &server.build_profile,
    );
    push_build_difference(
        &mut differences,
        "git_commit",
        client.git_commit.as_deref().unwrap_or("unknown"),
        server.git_commit.as_deref().unwrap_or("unknown"),
    );
    push_build_difference(
        &mut differences,
        "git_dirty",
        if client.git_dirty { "true" } else { "false" },
        if server.git_dirty { "true" } else { "false" },
    );

    let matches = differences.is_empty();

    BuildComparison {
        client: client.clone(),
        differences,
        matches,
        server: server.clone(),
    }
}

#[inline]
fn push_build_difference(
    differences: &mut Vec<BuildFieldDifference>,
    field: &str,
    client: &str,
    server: &str,
) {
    if client == server {
        return;
    }

    differences.push(BuildFieldDifference {
        client: client.to_owned(),
        field: field.to_owned(),
        server: server.to_owned(),
    });
}

#[inline]
#[must_use]
pub fn scope_summary(build: &BuildInfo) -> Value {
    json!({
        "name": SERVICE_NAME,
        "build": build,
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
pub fn epic_entity_ref(epic_id: Uuid) -> String {
    EntityRef::Epic(epic_id).to_string()
}

#[inline]
#[must_use]
pub fn parse_entity_ref(input: &str) -> Option<EntityRef> {
    let (kind, raw_id) = input.split_once(':')?;
    let id = Uuid::parse_str(raw_id).ok()?;
    match kind {
        "epic" => Some(EntityRef::Epic(id)),
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
