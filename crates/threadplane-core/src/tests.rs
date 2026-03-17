use proptest::prelude::{any, Strategy};
use proptest::{prop_assert, prop_assert_eq};
use rstest::rstest;
use uuid::Uuid;

use crate::{
    build_info, compare_build_info, default_config_path, default_system_config_path,
    epic_entity_ref, note_entity_ref, parse_entity_ref, relation_type, scope_summary,
    service_snapshot, task_entity_ref, EntityRef, EventKind, ThreadplaneConfig,
};

fn relation_inputs() -> impl Strategy<Value = String> {
    any::<String>()
}

fn uuid_inputs() -> impl Strategy<Value = Uuid> {
    any::<[u8; 16]>().prop_map(Uuid::from_bytes)
}

#[rstest]
#[case(
    "epic:550e8400-e29b-41d4-a716-446655440000",
    Some(EntityRef::Epic(Uuid::from_u128(0x550e_8400_e29b_41d4_a716_4466_5544_0000,)))
)]
#[case(
    "note:550e8400-e29b-41d4-a716-446655440000",
    Some(EntityRef::Note(Uuid::from_u128(0x550e_8400_e29b_41d4_a716_4466_5544_0000,)))
)]
#[case(
    "task:550e8400-e29b-41d4-a716-446655440000",
    Some(EntityRef::Task(Uuid::from_u128(0x550e_8400_e29b_41d4_a716_4466_5544_0000,)))
)]
#[case("weird:550e8400-e29b-41d4-a716-446655440000", None)]
#[case("note:not-a-uuid", None)]
#[case("task", None)]
fn parse_entity_ref_handles_supported_shapes(
    #[case] input: &str,
    #[case] expected: Option<EntityRef>,
) {
    assert_eq!(parse_entity_ref(input), expected);
}

#[rstest]
#[case("depends_on", "DEPENDS_ON")]
#[case("blocked by", "BLOCKED_BY")]
#[case("  mixed-Case relation ", "MIXED_CASE_RELATION")]
#[case("///", "")]
fn relation_type_normalizes_examples(#[case] input: &str, #[case] expected: &str) {
    assert_eq!(relation_type(input), expected);
}

#[rstest]
#[case(EventKind::FactPromoted)]
#[case(EventKind::LinkDeclared)]
#[case(EventKind::EpicRecorded)]
#[case(EventKind::NoteRecorded)]
#[case(EventKind::NoteUpdated)]
#[case(EventKind::TaskClaimed)]
#[case(EventKind::TaskCompleted)]
#[case(EventKind::TaskDependencyDeclared)]
#[case(EventKind::TaskOffered)]
#[case(EventKind::TaskReleased)]
#[case(EventKind::TaskUpdated)]
#[case(EventKind::XanaduLinked)]
fn service_snapshot_advertises_all_supported_event_kinds(#[case] kind: EventKind) {
    let snapshot = service_snapshot(build_info(
        "threadplane-server",
        "0.1.0",
        "debug",
        Some("abcdef123456"),
        false,
    ));
    assert!(snapshot.event_kinds.contains(&kind));
}

#[test]
fn service_snapshot_embeds_build_identity() {
    let snapshot = service_snapshot(build_info(
        "threadplane-server",
        "0.1.0",
        "release",
        Some("abcdef123456"),
        true,
    ));

    assert_eq!(snapshot.build.service, "threadplane-server");
    assert_eq!(snapshot.build.version, "0.1.0");
    assert_eq!(snapshot.build.build_profile, "release");
    assert_eq!(snapshot.build.git_commit.as_deref(), Some("abcdef123456"));
    assert!(snapshot.build.git_dirty);
}

#[test]
fn scope_summary_embeds_build_identity() {
    let build_identity = build_info(
        "threadplane-server",
        "0.1.0",
        "debug",
        Some("abcdef123456"),
        true,
    );
    let scope = scope_summary(&build_identity);
    let build_object = scope
        .get("build")
        .and_then(serde_json::Value::as_object);

    assert_eq!(
        build_object.and_then(|value| value.get("service")),
        Some(&serde_json::Value::from("threadplane-server"))
    );
    assert_eq!(
        build_object.and_then(|value| value.get("version")),
        Some(&serde_json::Value::from("0.1.0"))
    );
    assert_eq!(
        build_object.and_then(|value| value.get("build_profile")),
        Some(&serde_json::Value::from("debug"))
    );
    assert_eq!(
        build_object.and_then(|value| value.get("git_commit")),
        Some(&serde_json::Value::from("abcdef123456"))
    );
    assert_eq!(
        build_object.and_then(|value| value.get("git_dirty")),
        Some(&serde_json::Value::from(true))
    );
}

#[test]
fn compare_build_info_reports_field_differences() {
    let client = build_info(
        "threadplane-cli",
        "0.1.0",
        "debug",
        Some("aaaaaaaaaaaa"),
        true,
    );
    let server = build_info(
        "threadplane-server",
        "0.1.1",
        "release",
        Some("bbbbbbbbbbbb"),
        false,
    );

    let comparison = compare_build_info(&client, &server);
    let fields = comparison
        .differences
        .iter()
        .map(|difference| difference.field.as_str())
        .collect::<Vec<_>>();

    assert!(!comparison.matches);
    assert_eq!(comparison.differences.len(), 4);
    assert_eq!(fields, vec!["version", "build_profile", "git_commit", "git_dirty"]);
}

#[test]
fn threadplane_config_defaults_match_local_poc_expectations() {
    let config = ThreadplaneConfig::default();

    assert_eq!(config.cli.url, "http://127.0.0.1:4000");
    assert_eq!(config.server.bind, "127.0.0.1:4000");
    assert_eq!(config.server.default_lease_seconds, 300);
    assert_eq!(default_config_path().to_string_lossy(), "etc/config.toml");
    assert_eq!(
        default_system_config_path().to_string_lossy(),
        "/etc/threadplane/config.toml"
    );
    assert_eq!(config.server.database_url, None);
    assert_eq!(config.server.neo4j_password, None);
    assert_eq!(config.server.neo4j_uri, None);
    assert_eq!(config.server.neo4j_user, None);
}

proptest::proptest! {
    #[test]
    fn formatted_epic_refs_round_trip(epic_id in uuid_inputs()) {
        let entity_ref = epic_entity_ref(epic_id);
        prop_assert_eq!(parse_entity_ref(&entity_ref), Some(EntityRef::Epic(epic_id)));
    }

    #[test]
    fn formatted_note_refs_round_trip(note_id in uuid_inputs()) {
        let entity_ref = note_entity_ref(note_id);
        prop_assert_eq!(parse_entity_ref(&entity_ref), Some(EntityRef::Note(note_id)));
    }

    #[test]
    fn formatted_task_refs_round_trip(task_id in uuid_inputs()) {
        let entity_ref = task_entity_ref(task_id);
        prop_assert_eq!(parse_entity_ref(&entity_ref), Some(EntityRef::Task(task_id)));
    }

    #[test]
    fn relation_type_is_idempotent(input in relation_inputs()) {
        let normalized = relation_type(&input);
        prop_assert_eq!(relation_type(&normalized), normalized);
    }

    #[test]
    fn relation_type_only_emits_uppercase_ascii_word_separators(input in relation_inputs()) {
        let normalized = relation_type(&input);
        let has_invalid_char = normalized
            .chars()
            .any(|character| !character.is_ascii_uppercase() && !character.is_ascii_digit() && character != '_');

        prop_assert!(!has_invalid_char);
        prop_assert!(!normalized.starts_with('_'));
        prop_assert!(!normalized.ends_with('_'));
        prop_assert!(!normalized.contains("__"));
    }
}
