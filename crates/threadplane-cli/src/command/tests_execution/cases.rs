use serde_json::json;

use super::super::*;
use super::support::*;
use crate::runtime::CommandContext;
use threadplane_core::{build_info, ApiEnvelope, ServiceSnapshot, TaskSummary};

#[test]
fn execute_next_task_renders_compact_output_through_runtime() {
    let task_id = Uuid::parse_str("11111111-2222-3333-4444-555555555555").unwrap_or_default();
    let api = FakeApi::default().with_get_response(
        "/v1/workspaces/threadplane-dev/tasks/next?status=open&ready_only=true",
        &ApiEnvelope {
            ok: true,
            data: Some(TaskListEntry {
                active_claim: None,
                dependencies: Vec::new(),
                dependents: Vec::new(),
                epic: None,
                ready: true,
                task: TaskSummary {
                    author: "codex".to_owned(),
                    created_at: "2026-03-18T00:00:00Z".to_owned(),
                    details: "Keep work flowing.".to_owned(),
                    entity_ref: format!("task:{task_id}"),
                    epic_id: None,
                    metadata: TaskMetadata {
                        labels: vec!["workflow".to_owned()],
                        owner: Some("codex".to_owned()),
                        priority: TaskPriority::from_lossy("high"),
                    },
                    status: "open".to_owned(),
                    task_id,
                    title: "Pick next ready task".to_owned(),
                    transclusion_id: None,
                    updated_at: "2026-03-18T00:00:00Z".to_owned(),
                    workspace: "threadplane-dev".to_owned(),
                },
            }),
            receipt: None,
        },
    );
    let mut output = FakeOutput::default();
    let sleeper = RecordingSleeper::default();
    let config = sample_config();
    let discovery = sample_discovery();
    let mut context = CommandContext::builder()
        .api(&api)
        .output(&mut output)
        .sleeper(&sleeper)
        .build();

    if let Err(error) = execute(
        Cli {
            command: Command::Task(TaskCommand {
                command: TaskSubcommand::Next(NextTask {
                    epic_id: None,
                    format: OutputFormat::Compact,
                    label: None,
                    metadata_filters: TaskMetadataFilterArgs::default(),
                    workspace: "threadplane-dev".to_owned(),
                }),
            }),
            config: None,
            idempotency_key: None,
            server: None,
        },
        &config,
        &discovery,
        &mut context,
    ) {
        panic!("next task command succeeds: {error}");
    }

    assert_eq!(
        api.requests(),
        vec!["GET /v1/workspaces/threadplane-dev/tasks/next?status=open&ready_only=true"]
    );
    assert!(output.rendered.contains("Pick next ready task"));
    assert!(output.rendered.contains("priority=high"));
    assert!(sleeper.sleeps().is_empty());
}

#[test]
fn execute_scope_uses_fake_ports_and_emits_build_warning() {
    let api = FakeApi::default()
        .with_get_response("/scope", &json!({"ok": true, "summary": "scope"}))
        .with_get_response(
            "/",
            &ServiceSnapshot {
                build: build_info(
                    "threadplane-server",
                    "9.9.9",
                    "release",
                    Some("bbbbbbbbbbbb"),
                    false,
                ),
                event_kinds: Vec::new(),
                graph_projection: "neo4j".to_owned(),
                name: "threadplane".to_owned(),
                source_of_truth: "postgres".to_owned(),
                summary: "shared memory".to_owned(),
                tuple_space: "lease-backed".to_owned(),
            },
        );
    let mut output = FakeOutput::default();
    let sleeper = RecordingSleeper::default();
    let config = sample_config();
    let discovery = sample_discovery();
    let mut context = CommandContext::builder()
        .api(&api)
        .output(&mut output)
        .sleeper(&sleeper)
        .build();

    if let Err(error) = execute(
        Cli {
            command: Command::Scope,
            config: None,
            idempotency_key: None,
            server: None,
        },
        &config,
        &discovery,
        &mut context,
    ) {
        panic!("scope command succeeds: {error}");
    }

    assert_eq!(api.requests(), vec!["GET /scope", "GET /"]);
    assert!(output.rendered.contains("\"summary\": \"scope\""));
    assert_eq!(output.warnings.len(), 1);
    let first_warning = output.warnings.first().cloned().unwrap_or_default();
    assert!(first_warning.contains("warning:"));
    assert!(sleeper.sleeps().is_empty());
}
