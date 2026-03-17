use chrono::Utc;
use proptest::arbitrary::any;
use proptest::prop_assert_eq;
use rstest::rstest;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::{
    build_info::current_build_info,
    handlers::normalized_list_limit,
    lifecycle::{
        calculate_claim_expiry, normalized_lease_seconds, wait_for_shutdown, MINIMUM_LEASE_SECONDS,
    },
    migration::{
        COMMAND_RECEIPTS_MIGRATION_SQL, INITIAL_MIGRATION_SQL, PROJECTION_OFFSETS_MIGRATION_SQL,
    },
    projections::deduplicate_graph_relations,
    storage::{
        build_projection_status, event_kind_name, parse_event_kind, task_priority_rank,
        ProjectionCursor,
    },
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
fn command_receipts_migration_covers_idempotent_command_storage(
    #[case] expected_fragment: &str,
) {
    let has_fragment = COMMAND_RECEIPTS_MIGRATION_SQL.contains(expected_fragment);
    assert!(
        has_fragment,
        "missing migration fragment: {expected_fragment}"
    );
}

#[rstest]
#[case(TaskPriority::Low, 0)]
#[case(TaskPriority::Medium, 1)]
#[case(TaskPriority::High, 2)]
#[case(TaskPriority::Urgent, 3)]
fn task_priority_rank_orders_ready_queues(#[case] priority: TaskPriority, #[case] expected: u8) {
    assert_eq!(task_priority_rank(priority), expected);
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
