use super::*;

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
    "memory:550e8400-e29b-41d4-a716-446655440000",
    Some(EntityRef::Memory(Uuid::from_u128(0x550e_8400_e29b_41d4_a716_4466_5544_0000,)))
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
    fn formatted_memory_refs_round_trip(memory_id in uuid_inputs()) {
        let entity_ref = memory_entity_ref(memory_id);
        prop_assert_eq!(parse_entity_ref(&entity_ref), Some(EntityRef::Memory(memory_id)));
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
