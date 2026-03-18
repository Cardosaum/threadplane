#![expect(
    clippy::panic,
    reason = "Test-only fakes use explicit panic messages for fixture setup failures."
)]

use std::path::PathBuf;

use alloc::collections::BTreeMap;
use core::{cell::RefCell, time::Duration};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::{json, Value};

use super::*;
use crate::runtime::{ApiClient, CommandContext, CommandOutput, Sleeper};
use threadplane_core::{
    build_info, CliConfig, PublicKeyAlgorithm, ServerConfig, ServiceSnapshot, TaskSummary,
    WorkspaceAuthPolicy, WorkspaceBootstrapConfig, WorkspaceBootstrapMembershipConfig,
    WorkspaceBootstrapPublicKeyConfig,
};

#[derive(Default)]
struct FakeApi {
    gets: BTreeMap<String, Value>,
    requests: RefCell<Vec<String>>,
}

impl FakeApi {
    fn with_get_response<T>(mut self, path: &str, value: &T) -> Self
    where
        T: Serialize,
    {
        let serialized = match serde_json::to_value(value) {
            Ok(serialized) => serialized,
            Err(error) => panic!("serializable fake response: {error}"),
        };
        self.gets.insert(path.to_owned(), serialized);
        self
    }

    fn requests(&self) -> Vec<String> {
        self.requests.borrow().clone()
    }
}

impl ApiClient for FakeApi {
    fn get_json<T>(&self, path: &str) -> Result<T>
    where
        T: DeserializeOwned,
    {
        self.requests.borrow_mut().push(format!("GET {path}"));
        let value = self.gets.get(path).cloned().ok_or_else(|| {
            Usage {
                message: format!("missing fake GET response for {path}"),
            }
            .build()
        })?;

        serde_json::from_value(value).map_err(|source| {
            Usage {
                message: format!("failed to deserialize fake GET response for {path}: {source}"),
            }
            .build()
        })
    }

    fn patch_json<B, T>(&self, path: &str, _body: &B, _idempotency_key: Option<&str>) -> Result<T>
    where
        B: Serialize,
        T: DeserializeOwned,
    {
        Err(Usage {
            message: format!("unexpected fake PATCH {path}"),
        }
        .build())
    }

    fn post_json<B, T>(&self, path: &str, _body: &B, _idempotency_key: Option<&str>) -> Result<T>
    where
        B: Serialize,
        T: DeserializeOwned,
    {
        Err(Usage {
            message: format!("unexpected fake POST {path}"),
        }
        .build())
    }

    fn put_json<B, T>(&self, path: &str, _body: &B, _idempotency_key: Option<&str>) -> Result<T>
    where
        B: Serialize,
        T: DeserializeOwned,
    {
        Err(Usage {
            message: format!("unexpected fake PUT {path}"),
        }
        .build())
    }
}

#[derive(Default)]
struct FakeOutput {
    rendered: String,
    warnings: Vec<String>,
}

impl CommandOutput for FakeOutput {
    fn print(&mut self, text: &str) {
        self.rendered.push_str(text);
    }

    fn print_warning(&mut self, text: &str) {
        self.warnings.push(text.to_owned());
    }
}

#[derive(Default)]
struct RecordingSleeper {
    sleeps: RefCell<Vec<Duration>>,
}

impl RecordingSleeper {
    fn sleeps(&self) -> Vec<Duration> {
        self.sleeps.borrow().clone()
    }
}

impl Sleeper for RecordingSleeper {
    fn sleep(&self, duration: Duration) {
        self.sleeps.borrow_mut().push(duration);
    }
}

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

fn sample_config() -> ThreadplaneConfig {
    ThreadplaneConfig {
        cli: CliConfig {
            url: "http://127.0.0.1:4000".to_owned(),
        },
        server: ServerConfig {
            bind: "127.0.0.1:4000".to_owned(),
            database_url: "postgres://threadplane:test@127.0.0.1/threadplane".to_owned(),
            default_lease_seconds: 120,
            neo4j_password: "test".to_owned(),
            neo4j_uri: "bolt://127.0.0.1:7687".to_owned(),
            neo4j_user: "neo4j".to_owned(),
            workspace_bootstrap: WorkspaceBootstrapConfig {
                auth: WorkspaceAuthPolicy {
                    allowed_algorithms: vec![
                        PublicKeyAlgorithm::Ed25519,
                        PublicKeyAlgorithm::SshEd25519,
                    ],
                    challenge_ttl_seconds: 300,
                    signed_commands_required: false,
                },
                memberships: vec![WorkspaceBootstrapMembershipConfig {
                    actor_id: "operator".to_owned(),
                    role: WorkspaceRole::Admin,
                }],
                priorities: WorkspacePriorityPolicy {
                    default_priority: "medium".to_owned(),
                    priorities: vec![
                        WorkspacePriority {
                            description: Some("Important".to_owned()),
                            name: "high".to_owned(),
                            rank: 10,
                        },
                        WorkspacePriority {
                            description: Some("Normal".to_owned()),
                            name: "medium".to_owned(),
                            rank: 20,
                        },
                    ],
                },
                public_keys: vec![WorkspaceBootstrapPublicKeyConfig {
                    actor_id: "operator".to_owned(),
                    algorithm: PublicKeyAlgorithm::Ed25519,
                    key_id: "operator-main".to_owned(),
                    public_key: "ed25519:test".to_owned(),
                }],
            },
        },
    }
}

fn sample_discovery() -> ConfigDiscovery {
    ConfigDiscovery {
        env_override: None,
        env_prefix: "THREADPLANE",
        explicit_override: None,
        search_order: vec![PathBuf::from("/tmp/threadplane/config.toml")],
        selected_path: Some(PathBuf::from("/tmp/threadplane/config.toml")),
    }
}
