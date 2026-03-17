use std::io::Error as IoError;

use snafu::IntoError as _;
use uuid::Uuid;

use crate::command::{
    build_mismatch_warning, dedup_task_ids, entity_relations_path, entity_show_path,
    events_list_path, events_tail_path, note_list_path, render_entity_context_compact,
    render_event_list_compact, render_graph_relations_compact, render_note_list_compact,
    render_task_dependency_compact, render_task_list_compact, triage_has_changes,
};
use crate::error::{ContractMismatchDetails, JsonContractMismatch};
use threadplane_core::{
    build_info, compare_build_info, EntityContext, EntityRecord, EpicRecord, EventKind,
    EventRecord, GraphRelation, NoteRecord, TaskClaimRecord, TaskDependencySummary, TaskListEntry,
    TaskMetadata, TaskPriority, TaskSummary,
};

fn sample_task_metadata() -> TaskMetadata {
    TaskMetadata {
        labels: vec!["workflow".to_owned(), "agent".to_owned()],
        owner: Some("codex".to_owned()),
        priority: TaskPriority::High,
    }
}

#[test]
fn build_mismatch_warning_lists_changed_fields() {
    let client = build_info("threadplane-cli", "0.1.0", "debug", Some("aaaaaaaaaaaa"), true);
    let server = build_info(
        "threadplane-server",
        "0.1.1",
        "release",
        Some("bbbbbbbbbbbb"),
        false,
    );
    let comparison = compare_build_info(&client, &server);

    let warning_message = build_mismatch_warning(&comparison);

    assert!(warning_message.is_some());
    let warning_text = warning_message.unwrap_or_default();
    assert!(warning_text.contains("changed fields: version, build_profile, git_commit, git_dirty"));
}

#[test]
fn build_mismatch_warning_is_absent_when_builds_match() {
    let client = build_info("threadplane-cli", "0.1.0", "debug", Some("aaaaaaaaaaaa"), true);
    let server = build_info("threadplane-cli", "0.1.0", "debug", Some("aaaaaaaaaaaa"), true);
    let comparison = compare_build_info(&client, &server);

    assert!(build_mismatch_warning(&comparison).is_none());
}

#[test]
fn contract_mismatch_error_mentions_build_compare_guidance() {
    let error = JsonContractMismatch {
        details: Box::new(ContractMismatchDetails {
            changed_fields: "version, git_commit".to_owned(),
            cli_commit: "aaaaaaaaaaaa".to_owned(),
            cli_version: "0.1.0".to_owned(),
            server_commit: "bbbbbbbbbbbb".to_owned(),
            server_version: "0.2.0".to_owned(),
        }),
        url: "http://127.0.0.1:4000/v1/workspaces/threadplane-dev/tasks".to_owned(),
    }
    .into_error(serde_json::Error::io(IoError::other("missing field `labels`")));

    let rendered = error.to_string();

    assert!(rendered.contains("different contract"));
    assert!(rendered.contains("Run `threadplane build compare`"));
    assert!(rendered.contains("0.1.0"));
    assert!(rendered.contains("0.2.0"));
}

#[test]
fn render_task_list_compact_formats_ready_tasks() {
    let task_id = Uuid::parse_str("11111111-2222-3333-4444-555555555555").unwrap_or_default();
    let rendered = render_task_list_compact(&[TaskListEntry {
        active_claim: Some(TaskClaimRecord {
            actor: "codex".to_owned(),
            claim_id: Uuid::parse_str("aaaaaaaa-2222-3333-4444-555555555555").unwrap_or_default(),
            claimed_at: "2026-03-17T04:25:00Z".to_owned(),
            event_id: Uuid::parse_str("bbbbbbbb-2222-3333-4444-555555555555").unwrap_or_default(),
            expires_at: "2026-03-17T04:30:00Z".to_owned(),
            task_id,
            workspace: "threadplane-dev".to_owned(),
        }),
        dependencies: vec![TaskDependencySummary {
            depth: 1,
            entity_ref: "task:aaaaaaaa-0000-0000-0000-000000000000".to_owned(),
            status: "completed".to_owned(),
            task_id: Uuid::parse_str("aaaaaaaa-0000-0000-0000-000000000000").unwrap_or_default(),
            title: "Ship durable task lifecycle".to_owned(),
        }],
        dependents: Vec::new(),
        epic: Some(EpicRecord {
            author: "operator".to_owned(),
            body: "Dogfood the repo itself.".to_owned(),
            created_at: "2026-03-17T04:00:00Z".to_owned(),
            entity_ref: "epic:aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee".to_owned(),
            epic_id: Uuid::parse_str("aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee").unwrap_or_default(),
            event_id: Uuid::parse_str("cccccccc-bbbb-cccc-dddd-eeeeeeeeeeee").unwrap_or_default(),
            title: "Dogfooding".to_owned(),
            updated_at: "2026-03-17T04:00:00Z".to_owned(),
            workspace: "threadplane-dev".to_owned(),
        }),
        ready: true,
        task: TaskSummary {
            author: "codex".to_owned(),
            created_at: "2026-03-17T04:00:00Z".to_owned(),
            details: "Keep the queue readable.".to_owned(),
            entity_ref: "task:11111111-2222-3333-4444-555555555555".to_owned(),
            epic_id: Some(
                Uuid::parse_str("aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee").unwrap_or_default(),
            ),
            metadata: sample_task_metadata(),
            status: "open".to_owned(),
            task_id,
            title: "Add concise ready-queue views".to_owned(),
            transclusion_id: None,
            updated_at: "2026-03-17T04:00:00Z".to_owned(),
            workspace: "threadplane-dev".to_owned(),
        },
    }]);

    assert!(rendered.contains("11111111 | Add concise ready-queue views"));
    assert!(rendered.contains("status=open"));
    assert!(rendered.contains("priority=high"));
    assert!(rendered.contains("ready"));
    assert!(rendered.contains("deps=1"));
    assert!(rendered.contains("epic=Dogfooding"));
    assert!(rendered.contains("owner=codex"));
    assert!(rendered.contains("labels=workflow,agent"));
    assert!(rendered.contains("claim=codex"));
}

#[test]
fn render_task_list_compact_handles_empty_lists() {
    assert_eq!(render_task_list_compact(&[]), "no tasks\n");
}

#[test]
fn render_task_dependency_compact_formats_entries() {
    let rendered = render_task_dependency_compact(&[TaskDependencySummary {
        depth: 2,
        entity_ref: "task:aaaaaaaa-0000-0000-0000-000000000000".to_owned(),
        status: "completed".to_owned(),
        task_id: Uuid::parse_str("aaaaaaaa-0000-0000-0000-000000000000").unwrap_or_default(),
        title: "Ship durable task lifecycle".to_owned(),
    }]);

    assert!(rendered.contains("aaaaaaaa | Ship durable task lifecycle"));
    assert!(rendered.contains("status=completed"));
    assert!(rendered.contains("depth=2"));
}

#[test]
fn render_task_dependency_compact_handles_empty_lists() {
    assert_eq!(render_task_dependency_compact(&[]), "no tasks\n");
}

#[test]
fn render_note_list_compact_formats_entries() {
    let rendered = render_note_list_compact(&[NoteRecord {
        author: "codex".to_owned(),
        body: "Lease notes".to_owned(),
        created_at: "2026-03-17T04:00:00Z".to_owned(),
        entity_ref: "note:aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee".to_owned(),
        event_id: Uuid::parse_str("aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee").unwrap_or_default(),
        note_id: Uuid::parse_str("11111111-2222-3333-4444-555555555555").unwrap_or_default(),
        title: "Lease design".to_owned(),
        transclusion_id: None,
        updated_at: "2026-03-17T04:05:00Z".to_owned(),
        workspace: "threadplane-dev".to_owned(),
    }]);

    assert!(rendered.contains("11111111 | Lease design | author=codex"));
    assert!(rendered.contains("updated_at=2026-03-17T04:05:00Z"));
}

#[test]
fn render_event_list_compact_formats_entries() {
    let rendered = render_event_list_compact(&[EventRecord {
        actor: "codex".to_owned(),
        created_at: "2026-03-17T04:05:00Z".to_owned(),
        event_id: Uuid::parse_str("aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee").unwrap_or_default(),
        kind: EventKind::TaskOffered,
        payload: serde_json::json!({"task_id": "11111111-2222-3333-4444-555555555555"}),
        workspace: "threadplane-dev".to_owned(),
    }]);

    assert!(rendered.contains("aaaaaaaa | task_offered | actor=codex"));
    assert!(rendered.contains("at=2026-03-17T04:05:00Z"));
}

#[test]
fn render_graph_relations_compact_formats_entries() {
    let rendered = render_graph_relations_compact(&[GraphRelation {
        body: Some("Shared note".to_owned()),
        direction: "incoming".to_owned(),
        entity_kind: "note".to_owned(),
        entity_ref: "note:11111111-2222-3333-4444-555555555555".to_owned(),
        relation: "XANADU_LINK".to_owned(),
        title: Some("Lease wording".to_owned()),
        transclusion_id: None,
    }]);

    assert!(rendered.contains("incoming XANADU_LINK"));
    assert!(rendered.contains("note:11111111"));
    assert!(rendered.contains("Lease wording"));
}

#[test]
fn render_entity_context_compact_formats_task_summary_and_relations() {
    let rendered = render_entity_context_compact(&EntityContext {
        entity: EntityRecord::Epic(EpicRecord {
            author: "operator".to_owned(),
            body: "Dogfood the repo itself.".to_owned(),
            created_at: "2026-03-17T04:00:00Z".to_owned(),
            entity_ref: "epic:aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee".to_owned(),
            epic_id: Uuid::parse_str("aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee").unwrap_or_default(),
            event_id: Uuid::parse_str("cccccccc-bbbb-cccc-dddd-eeeeeeeeeeee").unwrap_or_default(),
            title: "Dogfooding".to_owned(),
            updated_at: "2026-03-17T04:00:00Z".to_owned(),
            workspace: "threadplane-dev".to_owned(),
        }),
        relations: vec![GraphRelation {
            body: Some("Shared note".to_owned()),
            direction: "outgoing".to_owned(),
            entity_kind: "task".to_owned(),
            entity_ref: "task:11111111-2222-3333-4444-555555555555".to_owned(),
            relation: "IMPLEMENTS_EPIC".to_owned(),
            title: Some("Ship durable task lifecycle".to_owned()),
            transclusion_id: None,
        }],
    });

    assert!(rendered.contains("epic aaaaaaaa | Dogfooding"));
    assert!(rendered.contains("outgoing IMPLEMENTS_EPIC"));
    assert!(rendered.contains("task:11111111"));
}

#[test]
fn note_list_path_applies_optional_filters() {
    let path = note_list_path(
        "threadplane-dev",
        Some(10),
        Some(" codex "),
        Some("lease"),
    );

    assert_eq!(
        path,
        "/v1/workspaces/threadplane-dev/notes?limit=10&author=codex&query=lease"
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

#[test]
fn dedup_task_ids_keeps_unique_sorted_values() {
    let low = Uuid::parse_str("11111111-2222-3333-4444-555555555555").unwrap_or_default();
    let high = Uuid::parse_str("99999999-2222-3333-4444-555555555555").unwrap_or_default();

    assert_eq!(dedup_task_ids(&[high, low, high]), vec![low, high]);
}

#[test]
fn triage_has_changes_rejects_noop_requests() {
    let noop = super::command::TaskMetadataPatchArgs::default();
    let priority_change = super::command::TaskMetadataPatchArgs {
        priority: Some(super::command::TaskPriorityValue::Urgent),
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
