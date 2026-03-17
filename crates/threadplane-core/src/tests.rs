use proptest::prelude::{any, Strategy};
use proptest::{prop_assert, prop_assert_eq};
use rstest::rstest;
use uuid::Uuid;

use crate::{
    default_config_path, default_system_config_path, note_entity_ref, parse_entity_ref,
    relation_type, service_snapshot, task_entity_ref, EntityRef, EventKind, ThreadplaneConfig,
};

fn relation_inputs() -> impl Strategy<Value = String> {
    any::<String>()
}

fn uuid_inputs() -> impl Strategy<Value = Uuid> {
    any::<[u8; 16]>().prop_map(Uuid::from_bytes)
}

#[rstest]
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
#[case(EventKind::NoteRecorded)]
#[case(EventKind::NoteUpdated)]
#[case(EventKind::TaskClaimed)]
#[case(EventKind::TaskOffered)]
#[case(EventKind::TaskReleased)]
#[case(EventKind::TaskUpdated)]
#[case(EventKind::XanaduLinked)]
fn service_snapshot_advertises_all_supported_event_kinds(#[case] kind: EventKind) {
    let snapshot = service_snapshot();
    assert!(snapshot.event_kinds.contains(&kind));
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
