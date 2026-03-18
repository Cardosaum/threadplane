use core::sync::atomic::{AtomicUsize, Ordering};

use proptest::arbitrary::any;
use proptest::prop_assert_eq;
use rstest::rstest;
use tokio::sync::Barrier;
use tokio_util::sync::CancellationToken;

use crate::{
    build_info::current_build_info,
    handlers::normalized_list_limit,
    lifecycle::{
        calculate_claim_expiry, normalized_lease_seconds, wait_for_shutdown, MINIMUM_LEASE_SECONDS,
    },
    migration::{
        COMMAND_RECEIPTS_MIGRATION_SQL, INITIAL_MIGRATION_SQL, MEMORIES_MIGRATION_SQL,
        PERFORMANCE_INDEXES_MIGRATION_SQL, PROJECTION_OFFSETS_MIGRATION_SQL,
        WORKSPACE_GOVERNANCE_MIGRATION_SQL,
    },
    prelude::*,
    projections::deduplicate_graph_relations,
    storage::{build_projection_status, event_kind_name, parse_event_kind, ProjectionCursor},
};
use threadplane_core::{EventKind, GraphRelation, TaskPriority};

#[test]
fn current_build_info_reports_compiled_server_identity() {
    let build = current_build_info();
    let expected_dirty = matches!(env!("THREADPLANE_GIT_DIRTY"), "true");

    assert_eq!(build.service, "threadplane-server");
    assert_eq!(build.version, env!("CARGO_PKG_VERSION"));
    assert_eq!(build.build_profile, env!("THREADPLANE_BUILD_PROFILE"));
    assert_eq!(build.git_dirty, expected_dirty);
    assert!(!build.build_profile.is_empty());
}

#[rstest]
#[case(None, 300, 300)]
#[case(Some(5), 300, MINIMUM_LEASE_SECONDS)]
#[case(Some(30), 300, 30)]
#[case(Some(120), 300, 120)]
fn normalized_lease_seconds_enforces_minimum(
    #[case] requested_lease_seconds: Option<i64>,
    #[case] default_lease_seconds: i64,
    #[case] expected: i64,
) {
    assert_eq!(
        normalized_lease_seconds(requested_lease_seconds, default_lease_seconds),
        expected
    );
}

#[rstest]
#[case(None, 25)]
#[case(Some(0), 1)]
#[case(Some(7), 7)]
#[case(Some(500), 200)]
fn normalized_list_limit_clamps_bounds(#[case] requested: Option<i64>, #[case] expected: i64) {
    assert_eq!(normalized_list_limit(requested), expected);
}

#[rstest]
#[case(EventKind::FactPromoted, "fact_promoted")]
#[case(EventKind::LinkDeclared, "link_declared")]
#[case(EventKind::EpicRecorded, "epic_recorded")]
#[case(EventKind::MemoryRecorded, "memory_recorded")]
#[case(EventKind::NoteRecorded, "note_recorded")]
#[case(EventKind::NoteUpdated, "note_updated")]
#[case(EventKind::TaskClaimed, "task_claimed")]
#[case(EventKind::TaskCompleted, "task_completed")]
#[case(EventKind::TaskDependencyDeclared, "task_dependency_declared")]
#[case(EventKind::TaskOffered, "task_offered")]
#[case(EventKind::TaskReleased, "task_released")]
#[case(EventKind::TaskUpdated, "task_updated")]
#[case(EventKind::XanaduLinked, "xanadu_linked")]
fn event_kind_round_trips_through_storage_names(
    #[case] kind: EventKind,
    #[case] expected_storage_name: &str,
) {
    assert_eq!(event_kind_name(kind), expected_storage_name);
    assert_eq!(parse_event_kind(expected_storage_name), kind);
}

#[rstest]
#[case("CREATE TABLE IF NOT EXISTS events")]
#[case("CREATE TABLE IF NOT EXISTS epics")]
#[case("CREATE TABLE IF NOT EXISTS notes")]
#[case("CREATE TABLE IF NOT EXISTS tasks")]
#[case("CREATE TABLE IF NOT EXISTS task_claims")]
#[case("CREATE TABLE IF NOT EXISTS task_dependencies")]
#[case("CREATE TABLE IF NOT EXISTS links")]
#[case("CREATE TABLE IF NOT EXISTS transclusion_groups")]
#[case("ALTER TABLE notes ADD COLUMN IF NOT EXISTS transclusion_id UUID")]
#[case("ALTER TABLE tasks ADD COLUMN IF NOT EXISTS epic_id UUID")]
#[case("ALTER TABLE tasks ADD COLUMN IF NOT EXISTS priority TEXT NOT NULL DEFAULT 'medium'")]
#[case("ALTER TABLE tasks ADD COLUMN IF NOT EXISTS owner TEXT")]
#[case("ALTER TABLE tasks ADD COLUMN IF NOT EXISTS labels TEXT[] NOT NULL DEFAULT '{}'")]
#[case("ALTER TABLE tasks ADD COLUMN IF NOT EXISTS transclusion_id UUID")]
fn initial_migration_covers_poc_storage_surfaces(#[case] expected_fragment: &str) {
    let has_fragment = INITIAL_MIGRATION_SQL.contains(expected_fragment);
    assert!(
        has_fragment,
        "missing migration fragment: {expected_fragment}"
    );
}

#[rstest]
#[case("CREATE TABLE IF NOT EXISTS projection_offsets")]
#[case("projection_name TEXT PRIMARY KEY")]
#[case("last_event_created_at TIMESTAMPTZ")]
#[case("last_event_id UUID")]
fn projection_offsets_migration_covers_replay_cursor_storage(#[case] expected_fragment: &str) {
    let has_fragment = PROJECTION_OFFSETS_MIGRATION_SQL.contains(expected_fragment);
    assert!(
        has_fragment,
        "missing migration fragment: {expected_fragment}"
    );
}

#[rstest]
#[case("CREATE TABLE IF NOT EXISTS command_receipts")]
#[case("idempotency_key TEXT NOT NULL")]
#[case("request_payload JSONB NOT NULL")]
#[case("response_payload JSONB")]
#[case("UNIQUE (workspace, actor, command_kind, idempotency_key)")]
fn command_receipts_migration_covers_idempotent_command_storage(#[case] expected_fragment: &str) {
    let has_fragment = COMMAND_RECEIPTS_MIGRATION_SQL.contains(expected_fragment);
    assert!(
        has_fragment,
        "missing migration fragment: {expected_fragment}"
    );
}

#[rstest]
#[case("CREATE INDEX IF NOT EXISTS idx_events_created_at_event_id")]
#[case("CREATE INDEX IF NOT EXISTS idx_epics_event_id")]
#[case("CREATE INDEX IF NOT EXISTS idx_notes_event_id")]
#[case("CREATE INDEX IF NOT EXISTS idx_tasks_event_id")]
#[case("CREATE INDEX IF NOT EXISTS idx_task_claims_event_id")]
#[case("CREATE INDEX IF NOT EXISTS idx_links_event_id")]
#[case("CREATE INDEX IF NOT EXISTS idx_task_dependencies_event_id")]
#[case("CREATE INDEX IF NOT EXISTS idx_task_claims_active_task_claimed_at")]
fn performance_indexes_migration_covers_replay_and_claim_lookups(#[case] expected_fragment: &str) {
    let has_fragment = PERFORMANCE_INDEXES_MIGRATION_SQL.contains(expected_fragment);
    assert!(
        has_fragment,
        "missing migration fragment: {expected_fragment}"
    );
}

#[rstest]
#[case("CREATE TABLE IF NOT EXISTS workspace_policies")]
#[case("CREATE TABLE IF NOT EXISTS workspace_priorities")]
#[case("CREATE TABLE IF NOT EXISTS workspace_memberships")]
#[case("CREATE TABLE IF NOT EXISTS actor_public_keys")]
#[case("CREATE INDEX IF NOT EXISTS idx_workspace_priorities_workspace_rank")]
fn workspace_governance_migration_covers_policy_membership_and_key_storage(
    #[case] expected_fragment: &str,
) {
    let has_fragment = WORKSPACE_GOVERNANCE_MIGRATION_SQL.contains(expected_fragment);
    assert!(
        has_fragment,
        "missing migration fragment: {expected_fragment}"
    );
}

#[rstest]
#[case("CREATE TABLE IF NOT EXISTS memories")]
#[case("event_id UUID NOT NULL REFERENCES events(event_id) ON DELETE CASCADE")]
#[case("tags TEXT[] NOT NULL DEFAULT '{}'")]
#[case("recall_triggers TEXT[] NOT NULL DEFAULT '{}'")]
#[case("CREATE INDEX IF NOT EXISTS idx_memories_tags_gin")]
#[case("CREATE INDEX IF NOT EXISTS idx_memories_recall_triggers_gin")]
fn memories_migration_covers_structured_memory_storage(#[case] expected_fragment: &str) {
    let has_fragment = MEMORIES_MIGRATION_SQL.contains(expected_fragment);
    assert!(
        has_fragment,
        "missing migration fragment: {expected_fragment}"
    );
}

#[test]
fn task_priority_newtype_normalizes_storage_values() {
    assert_eq!(
        TaskPriority::new("Urgent Fix")
            .map(|priority| priority.to_string())
            .as_deref(),
        Some("urgent_fix")
    );
}

#[test]
fn projection_status_marks_caught_up_when_pending_is_zero() {
    let created_at = Utc::now();
    let cursor = ProjectionCursor::new(created_at, Uuid::nil());
    let status = build_projection_status("neo4j_graph", Some(cursor), 12, 0);

    assert!(status.caught_up);
    assert_eq!(status.projected_events, 12);
    assert_eq!(status.pending_events, 0);
    assert_eq!(status.last_event_id, Some(Uuid::nil()));
    assert_eq!(status.projection_name, "neo4j_graph");
}

#[test]
fn projection_status_reports_full_backlog_without_cursor() {
    let status = build_projection_status("neo4j_graph", None, 7, 7);

    assert!(!status.caught_up);
    assert_eq!(status.projected_events, 0);
    assert_eq!(status.pending_events, 7);
    assert_eq!(status.last_event_created_at, None);
    assert_eq!(status.last_event_id, None);
}

#[test]
fn deduplicate_graph_relations_collapses_replay_duplicates() {
    let relation = GraphRelation {
        body: Some("Shared text".to_owned()),
        direction: "incoming".to_owned(),
        entity_kind: "note".to_owned(),
        entity_ref: "note:00000000-0000-0000-0000-000000000000".to_owned(),
        relation: "XANADU_LINK".to_owned(),
        title: Some("Lease note".to_owned()),
        transclusion_id: Some(Uuid::nil()),
    };

    let deduplicated = deduplicate_graph_relations(vec![
        relation.clone(),
        relation,
        GraphRelation {
            body: Some("Dependency".to_owned()),
            direction: "outgoing".to_owned(),
            entity_kind: "task".to_owned(),
            entity_ref: "task:11111111-1111-1111-1111-111111111111".to_owned(),
            relation: "DEPENDS_ON".to_owned(),
            title: Some("Ship durable task lifecycle".to_owned()),
            transclusion_id: None,
        },
    ]);

    assert_eq!(deduplicated.len(), 2);
}

proptest::proptest! {
    #[test]
    fn calculated_claim_expiry_advances_by_requested_seconds(lease_seconds in MINIMUM_LEASE_SECONDS..10_000_i64) {
        let claimed_at = Utc::now();
        let expires_at = calculate_claim_expiry(claimed_at, lease_seconds);

        prop_assert_eq!(
            expires_at.map(|value| value.signed_duration_since(claimed_at).num_seconds()),
            Some(lease_seconds)
        );
    }
}

proptest::proptest! {
    #[test]
    fn projection_cursor_preserves_event_identity(event_bytes in any::<[u8; 16]>()) {
        let created_at = Utc::now();
        let event_id = Uuid::from_bytes(event_bytes);
        let cursor = ProjectionCursor::new(created_at, event_id);

        prop_assert_eq!(cursor.created_at, created_at);
        prop_assert_eq!(cursor.event_id, event_id);
    }
}

#[tokio::test]
async fn wait_for_shutdown_completes_after_cancellation() {
    let shutdown_token = CancellationToken::new();
    shutdown_token.cancel();

    wait_for_shutdown(shutdown_token).await;
}

#[tokio::test]
async fn projection_coordinator_serializes_concurrent_writes() {
    let projection_coordinator = ProjectionCoordinator::default();
    let shared_barrier = Arc::new(Barrier::new(2));
    let active_writers = Arc::new(AtomicUsize::new(0));
    let peak_writers = Arc::new(AtomicUsize::new(0));

    let first = {
        let first_barrier = Arc::clone(&shared_barrier);
        let first_active_writers = Arc::clone(&active_writers);
        let first_peak_writers = Arc::clone(&peak_writers);
        let first_projection_coordinator = projection_coordinator.clone();
        tokio::spawn(async move {
            first_projection_coordinator
                .run(async move {
                    let current = first_active_writers.fetch_add(1, Ordering::SeqCst) + 1;
                    first_peak_writers.fetch_max(current, Ordering::SeqCst);
                    first_barrier.wait().await;
                    first_active_writers.fetch_sub(1, Ordering::SeqCst);
                    Ok::<_, ThreadplaneServerError>(())
                })
                .await
        })
    };

    let second = {
        let second_barrier = Arc::clone(&shared_barrier);
        let second_active_writers = Arc::clone(&active_writers);
        let second_peak_writers = Arc::clone(&peak_writers);
        let second_projection_coordinator = projection_coordinator.clone();
        tokio::spawn(async move {
            second_barrier.wait().await;
            second_projection_coordinator
                .run(async move {
                    let current = second_active_writers.fetch_add(1, Ordering::SeqCst) + 1;
                    second_peak_writers.fetch_max(current, Ordering::SeqCst);
                    second_active_writers.fetch_sub(1, Ordering::SeqCst);
                    Ok::<_, ThreadplaneServerError>(())
                })
                .await
        })
    };

    let first_result = first.await;
    assert!(first_result.is_ok(), "first projection task should join");
    let Ok(first_projection_result) = first_result else {
        return;
    };
    assert!(
        first_projection_result.is_ok(),
        "first projection should succeed"
    );

    let second_result = second.await;
    assert!(second_result.is_ok(), "second projection task should join");
    let Ok(second_projection_result) = second_result else {
        return;
    };
    assert!(
        second_projection_result.is_ok(),
        "second projection should succeed"
    );

    assert_eq!(peak_writers.load(Ordering::SeqCst), 1);
}
