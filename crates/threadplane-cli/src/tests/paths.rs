use super::*;

#[test]
fn note_list_path_applies_optional_filters() {
    let path = note_list_path("threadplane-dev", Some(10), Some(" codex "), Some("lease"));

    assert_eq!(
        path,
        "/v1/workspaces/threadplane-dev/notes?limit=10&author=codex&query=lease"
    );
}

#[test]
fn memory_list_path_applies_structured_filters() {
    let path = memory_list_path(MemoryListPathArgs {
        audience: Some("agent"),
        importance: Some("critical"),
        kind: Some("Workflow Note"),
        limit: Some(10),
        query: Some("quality"),
        recall_trigger: Some("session start"),
        tag: Some("Prime"),
        workspace: "threadplane-dev",
    })
    .unwrap_or_default();

    assert_eq!(
        path,
        "/v1/workspaces/threadplane-dev/memories?limit=10&audience=agent&importance=critical&kind=workflow_note&query=quality&recall_trigger=session_start&tag=prime"
    );
}

#[test]
fn events_paths_match_workspace_reads() {
    let event_id = Uuid::parse_str("aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee").unwrap_or_default();

    assert_eq!(
        events_list_path("threadplane-dev", 15),
        "/v1/workspaces/threadplane-dev/events?limit=15"
    );
    assert_eq!(
        events_tail_path("threadplane-dev", 15, Some(event_id)),
        format!("/v1/workspaces/threadplane-dev/events/tail?limit=15&after_event_id={event_id}")
    );
}

#[test]
fn entity_paths_match_entity_reads() {
    let entity_ref = "task:aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee";

    assert_eq!(
        entity_show_path(entity_ref),
        "/v1/entities/task:aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee"
    );
    assert_eq!(
        entity_relations_path(entity_ref),
        "/v1/entities/task:aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee/relations"
    );
}

proptest! {
    #[test]
    fn events_tail_path_preserves_cursor_identity(event_bytes in any::<[u8; 16]>(), limit in 1_i64..500) {
        let event_id = Uuid::from_bytes(event_bytes);
        let path = events_tail_path("threadplane-dev", limit, Some(event_id));

        prop_assert_eq!(
            path,
            format!("/v1/workspaces/threadplane-dev/events/tail?limit={limit}&after_event_id={event_id}")
        );
    }
}
