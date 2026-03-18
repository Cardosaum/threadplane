use std::path::PathBuf;

use alloc::collections::BTreeMap;
use core::{cell::RefCell, time::Duration};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;

use super::super::*;
use crate::runtime::{ApiClient, CommandOutput, Sleeper};
use threadplane_core::{
    CliConfig, PublicKeyAlgorithm, ServerConfig, WorkspaceAuthPolicy, WorkspaceBootstrapConfig,
    WorkspaceBootstrapMembershipConfig, WorkspaceBootstrapPublicKeyConfig,
};

#[derive(Default)]
pub(super) struct FakeApi {
    gets: BTreeMap<String, Value>,
    requests: RefCell<Vec<String>>,
}

impl FakeApi {
    pub(super) fn with_get_response<T>(mut self, path: &str, value: &T) -> Self
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

    pub(super) fn requests(&self) -> Vec<String> {
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
pub(super) struct FakeOutput {
    pub(super) rendered: String,
    pub(super) warnings: Vec<String>,
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
pub(super) struct RecordingSleeper {
    sleeps: RefCell<Vec<Duration>>,
}

impl RecordingSleeper {
    pub(super) fn sleeps(&self) -> Vec<Duration> {
        self.sleeps.borrow().clone()
    }
}

impl Sleeper for RecordingSleeper {
    fn sleep(&self, duration: Duration) {
        self.sleeps.borrow_mut().push(duration);
    }
}

pub(super) fn sample_config() -> ThreadplaneConfig {
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

pub(super) fn sample_discovery() -> ConfigDiscovery {
    ConfigDiscovery {
        env_override: None,
        env_prefix: "THREADPLANE",
        explicit_override: None,
        search_order: vec![PathBuf::from("/tmp/threadplane/config.toml")],
        selected_path: Some(PathBuf::from("/tmp/threadplane/config.toml")),
    }
}
