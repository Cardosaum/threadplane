mod build;
mod config;
mod governance;
mod refs;

use core::error::Error;
use std::{io, path::PathBuf};

use figment::Jail;
use proptest::prelude::{any, Strategy};
use proptest::{prop_assert, prop_assert_eq};
use rstest::rstest;
use uuid::Uuid;

use crate::{
    build_info, compare_build_info, default_config_path, default_system_config_paths,
    discover_threadplane_config, epic_entity_ref, load_threadplane_config_with_overrides,
    load_threadplane_config_with_path, memory_entity_ref, normalize_memory_kind_name,
    normalize_memory_recall_triggers, normalize_memory_tags, normalize_task_labels,
    normalize_task_owner, normalize_workspace_priority_name, note_entity_ref, parse_entity_ref,
    relation_type, scope_summary, service_snapshot, task_entity_ref,
    validate_workspace_auth_policy, validate_workspace_policy, validate_workspace_priority_policy,
    CliConfigOverrides, EntityRef, EventKind, MemoryAudience, MemoryImportance, MemoryKind,
    MemoryScope, PublicKeyAlgorithm, TaskPriority, ThreadplaneConfigOverrides, WorkspaceAuthPolicy,
    WorkspacePolicy, WorkspacePriority, WorkspacePriorityPolicy, WorkspaceRole, ENV_PREFIX,
};

fn relation_inputs() -> impl Strategy<Value = String> {
    any::<String>()
}

fn uuid_inputs() -> impl Strategy<Value = Uuid> {
    any::<[u8; 16]>().prop_map(Uuid::from_bytes)
}

fn full_config_body() -> &'static str {
    r#"
[cli]
url = "http://127.0.0.1:4123"

[server]
bind = "127.0.0.1:4321"
database_url = "postgres://threadplane:secret@127.0.0.1:5432/threadplane"
default_lease_seconds = 42
neo4j_password = "neo4j-secret"
neo4j_uri = "127.0.0.1:7687"
neo4j_user = "neo4j"

[server.workspace_bootstrap.auth]
allowed_algorithms = ["ssh_ed25519"]
challenge_ttl_seconds = 90
signed_commands_required = true

[server.workspace_bootstrap.priorities]
default_priority = "normal"

[[server.workspace_bootstrap.priorities.priorities]]
name = "background"
rank = 10
description = "Useful but not urgent."

[[server.workspace_bootstrap.priorities.priorities]]
name = "normal"
rank = 20
description = "Expected day-to-day work."

[[server.workspace_bootstrap.priorities.priorities]]
name = "expedite"
rank = 30
description = "Pull forward ahead of normal backlog."

[[server.workspace_bootstrap.memberships]]
actor_id = "operator"
role = "admin"

[[server.workspace_bootstrap.public_keys]]
actor_id = "operator"
algorithm = "ssh_ed25519"
key_id = "local"
public_key = "ssh-ed25519 AAAATEST threadplane@example"
"#
}

fn sample_workspace_priority_policy() -> WorkspacePriorityPolicy {
    WorkspacePriorityPolicy {
        default_priority: "normal".to_owned(),
        priorities: vec![
            WorkspacePriority {
                name: "background".to_owned(),
                rank: 10,
                description: Some("Useful but not urgent.".to_owned()),
            },
            WorkspacePriority {
                name: "normal".to_owned(),
                rank: 20,
                description: Some("Expected day-to-day work.".to_owned()),
            },
            WorkspacePriority {
                name: "expedite".to_owned(),
                rank: 30,
                description: Some("Pull forward ahead of normal backlog.".to_owned()),
            },
        ],
    }
}
