use super::*;

#[test]
fn dedup_task_ids_keeps_unique_sorted_values() {
    let low = Uuid::parse_str("11111111-2222-3333-4444-555555555555").unwrap_or_default();
    let high = Uuid::parse_str("99999999-2222-3333-4444-555555555555").unwrap_or_default();

    assert_eq!(dedup_task_ids(&[high, low, high]), vec![low, high]);
}

proptest! {
    #[test]
    fn dedup_task_ids_matches_btreeset(values in vec(any::<[u8; 16]>(), 0..64)) {
        let uuids: Vec<_> = values.into_iter().map(Uuid::from_bytes).collect();
        let expected: Vec<_> = uuids.iter().copied().collect::<BTreeSet<_>>().into_iter().collect();

        prop_assert_eq!(dedup_task_ids(&uuids), expected);
    }
}

#[test]
fn triage_has_changes_rejects_noop_requests() {
    let noop = TaskMetadataPatchArgs::default();
    let priority_change = TaskMetadataPatchArgs {
        priority: Some("urgent".to_owned()),
        ..Default::default()
    };

    assert!(!triage_has_changes(false, None, &noop));
    assert!(triage_has_changes(true, None, &noop));
    assert!(triage_has_changes(
        false,
        Some(Uuid::parse_str("aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee").unwrap_or_default()),
        &noop,
    ));
    assert!(triage_has_changes(false, None, &priority_change));
}
