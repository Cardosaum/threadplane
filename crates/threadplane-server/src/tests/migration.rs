use super::*;

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
