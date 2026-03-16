use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

pub const SERVICE_NAME: &str = "threadplane";
pub const DEFAULT_BIND_ADDR: &str = "127.0.0.1:4000";
pub const DEFAULT_SERVER_URL: &str = "http://127.0.0.1:4000";
pub const DEFAULT_LEASE_SECONDS: i64 = 300;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    NoteRecorded,
    LinkDeclared,
    TaskOffered,
    TaskClaimed,
    TaskReleased,
    FactPromoted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventEnvelope {
    pub workspace: String,
    pub actor: String,
    pub kind: EventKind,
    pub payload: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceSnapshot {
    pub name: String,
    pub summary: String,
    pub source_of_truth: String,
    pub graph_projection: String,
    pub tuple_space: String,
    pub event_kinds: Vec<EventKind>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateNoteRequest {
    pub workspace: String,
    pub author: String,
    pub title: String,
    pub body: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OfferTaskRequest {
    pub workspace: String,
    pub author: String,
    pub title: String,
    pub details: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaimTaskRequest {
    pub workspace: String,
    pub actor: String,
    pub task_id: Uuid,
    pub lease_seconds: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddLinkRequest {
    pub workspace: String,
    pub actor: String,
    pub from: String,
    pub to: String,
    pub relation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteRecord {
    pub note_id: Uuid,
    pub entity_ref: String,
    pub event_id: Uuid,
    pub workspace: String,
    pub author: String,
    pub title: String,
    pub body: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRecord {
    pub task_id: Uuid,
    pub entity_ref: String,
    pub event_id: Uuid,
    pub workspace: String,
    pub author: String,
    pub title: String,
    pub details: String,
    pub status: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskClaimRecord {
    pub claim_id: Uuid,
    pub task_id: Uuid,
    pub workspace: String,
    pub actor: String,
    pub event_id: Uuid,
    pub claimed_at: String,
    pub expires_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkRecord {
    pub link_id: Uuid,
    pub event_id: Uuid,
    pub workspace: String,
    pub actor: String,
    pub from: String,
    pub to: String,
    pub relation: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventRecord {
    pub event_id: Uuid,
    pub workspace: String,
    pub actor: String,
    pub kind: EventKind,
    pub payload: Value,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSummary {
    pub task_id: Uuid,
    pub entity_ref: String,
    pub workspace: String,
    pub title: String,
    pub details: String,
    pub status: String,
    pub author: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphRelation {
    pub relation: String,
    pub direction: String,
    pub entity_ref: String,
    pub entity_kind: String,
    pub title: Option<String>,
    pub body: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskContext {
    pub task: TaskSummary,
    pub active_claim: Option<TaskClaimRecord>,
    pub relations: Vec<GraphRelation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiEnvelope<T> {
    pub ok: bool,
    pub data: T,
}

pub fn service_snapshot() -> ServiceSnapshot {
    ServiceSnapshot {
        name: SERVICE_NAME.to_string(),
        summary: "Shared human/agent memory and coordination plane".to_string(),
        source_of_truth: "PostgreSQL append-only event log managed by threadplane-server"
            .to_string(),
        graph_projection: "Neo4j projection for notes, dependencies, provenance, and traversal"
            .to_string(),
        tuple_space:
            "Service-managed tuple semantics with PostgreSQL persistence and lease-based claims"
                .to_string(),
        event_kinds: vec![
            EventKind::NoteRecorded,
            EventKind::LinkDeclared,
            EventKind::TaskOffered,
            EventKind::TaskClaimed,
            EventKind::TaskReleased,
            EventKind::FactPromoted,
        ],
    }
}

pub fn scope_summary() -> Value {
    json!({
        "name": SERVICE_NAME,
        "poc": {
            "goal": "Validate shared agent collaboration over an internet-reachable event log and graph projection",
            "service_boundary": "All writes pass through threadplane-server",
            "authoritative_log": "postgresql",
            "graph_projection": "neo4j",
            "tuple_coordination": "implemented in the service with postgres-backed leases",
            "future_influence": "VarveDB remains a candidate for local replicas and offline-first ingest buffers"
        }
    })
}

pub fn task_entity_ref(task_id: Uuid) -> String {
    format!("task:{task_id}")
}

pub fn note_entity_ref(note_id: Uuid) -> String {
    format!("note:{note_id}")
}

pub fn relation_type(input: &str) -> String {
    let mut relation = String::new();
    let mut last_was_underscore = false;

    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            relation.push(ch.to_ascii_uppercase());
            last_was_underscore = false;
        } else if !last_was_underscore {
            relation.push('_');
            last_was_underscore = true;
        }
    }

    relation.trim_matches('_').to_string()
}

#[cfg(test)]
mod tests {
    use super::relation_type;

    #[test]
    fn relation_type_normalizes_values() {
        assert_eq!(relation_type("depends_on"), "DEPENDS_ON");
        assert_eq!(relation_type("blocked by"), "BLOCKED_BY");
        assert_eq!(
            relation_type("  mixed-Case relation "),
            "MIXED_CASE_RELATION"
        );
    }
}
