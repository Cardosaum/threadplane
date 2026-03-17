use uuid::Uuid;

use crate::command::{build_mismatch_warning, render_task_list_compact};
use threadplane_core::{
    build_info, compare_build_info, EpicRecord, TaskClaimRecord, TaskDependencySummary,
    TaskListEntry, TaskSummary,
};

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
    assert!(rendered.contains("ready"));
    assert!(rendered.contains("deps=1"));
    assert!(rendered.contains("epic=Dogfooding"));
    assert!(rendered.contains("claim=codex"));
}

#[test]
fn render_task_list_compact_handles_empty_lists() {
    assert_eq!(render_task_list_compact(&[]), "no tasks\n");
}
