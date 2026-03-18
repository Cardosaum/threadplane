#![allow(
    clippy::wildcard_imports,
    reason = "Submodule reuses shared type imports via the crate-local prelude style"
)]

use super::*;

use serde_json::json;

use crate::config::SERVICE_NAME;

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
pub struct CommandReceipt {
    pub command_id: Uuid,
    pub command_kind: String,
    pub idempotency_key: String,
    pub recorded_at: String,
    pub replayed: bool,
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
