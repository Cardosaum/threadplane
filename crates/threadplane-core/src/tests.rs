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
    load_threadplane_config_with_path, normalize_task_labels, normalize_task_owner,
    normalize_workspace_priority_name, note_entity_ref, parse_entity_ref, relation_type,
    scope_summary, service_snapshot, task_entity_ref, validate_workspace_auth_policy,
    validate_workspace_policy, validate_workspace_priority_policy, CliConfigOverrides, EntityRef,
    EventKind, PublicKeyAlgorithm, TaskPriority, ThreadplaneConfigOverrides,
    WorkspaceAuthPolicy, WorkspacePolicy, WorkspacePriority, WorkspacePriorityPolicy,
    WorkspaceRole, ENV_PREFIX,
};

fn relation_inputs() -> impl Strategy<Value = String> {
    any::<String>()
}

fn uuid_inputs() -> impl Strategy<Value = Uuid> {
    any::<[u8; 16]>().prop_map(Uuid::from_bytes)
}

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
#[case(EventKind::EpicRecorded)]
#[case(EventKind::NoteRecorded)]
#[case(EventKind::NoteUpdated)]
#[case(EventKind::TaskClaimed)]
#[case(EventKind::TaskCompleted)]
#[case(EventKind::TaskDependencyDeclared)]
#[case(EventKind::TaskOffered)]
#[case(EventKind::TaskReleased)]
#[case(EventKind::TaskUpdated)]
#[case(EventKind::XanaduLinked)]
fn service_snapshot_advertises_all_supported_event_kinds(#[case] kind: EventKind) {
    let snapshot = service_snapshot(build_info(
        "threadplane-server",
        "0.1.0",
        "debug",
        Some("abcdef123456"),
        false,
    ));
    assert!(snapshot.event_kinds.contains(&kind));
}

#[test]
fn service_snapshot_embeds_build_identity() {
    let snapshot = service_snapshot(build_info(
        "threadplane-server",
        "0.1.0",
        "release",
        Some("abcdef123456"),
        true,
    ));

    assert_eq!(snapshot.build.service, "threadplane-server");
    assert_eq!(snapshot.build.version, "0.1.0");
    assert_eq!(snapshot.build.build_profile, "release");
    assert_eq!(snapshot.build.git_commit.as_deref(), Some("abcdef123456"));
    assert!(snapshot.build.git_dirty);
}

#[test]
fn scope_summary_embeds_build_identity() {
    let build_identity = build_info(
        "threadplane-server",
        "0.1.0",
        "debug",
        Some("abcdef123456"),
        true,
    );
    let scope = scope_summary(&build_identity);
    let build_object = scope
        .get("build")
        .and_then(serde_json::Value::as_object);

    assert_eq!(
        build_object.and_then(|value| value.get("service")),
        Some(&serde_json::Value::from("threadplane-server"))
    );
    assert_eq!(
        build_object.and_then(|value| value.get("version")),
        Some(&serde_json::Value::from("0.1.0"))
    );
    assert_eq!(
        build_object.and_then(|value| value.get("build_profile")),
        Some(&serde_json::Value::from("debug"))
    );
    assert_eq!(
        build_object.and_then(|value| value.get("git_commit")),
        Some(&serde_json::Value::from("abcdef123456"))
    );
    assert_eq!(
        build_object.and_then(|value| value.get("git_dirty")),
        Some(&serde_json::Value::from(true))
    );
}

#[test]
fn compare_build_info_reports_field_differences() {
    let client = build_info(
        "threadplane-cli",
        "0.1.0",
        "debug",
        Some("aaaaaaaaaaaa"),
        true,
    );
    let server = build_info(
        "threadplane-server",
        "0.1.1",
        "release",
        Some("bbbbbbbbbbbb"),
        false,
    );

    let comparison = compare_build_info(&client, &server);
    let fields = comparison
        .differences
        .iter()
        .map(|difference| difference.field.as_str())
        .collect::<Vec<_>>();

    assert!(!comparison.matches);
    assert_eq!(comparison.differences.len(), 4);
    assert_eq!(fields, vec!["version", "build_profile", "git_commit", "git_dirty"]);
}

#[test]
fn xdg_config_paths_use_threadplane_standard_locations() -> Result<(), Box<dyn Error>> {
    let user_path = default_config_path()?;
    let system_paths = default_system_config_paths()?;

    if !user_path.to_string_lossy().ends_with("threadplane/config.toml") {
        return Err(Box::new(io::Error::other("unexpected XDG user config path")));
    }
    if system_paths.is_empty() {
        return Err(Box::new(io::Error::other("missing XDG system config paths")));
    }
    if !system_paths
        .iter()
        .all(|path| path.to_string_lossy().ends_with("threadplane/config.toml"))
    {
        return Err(Box::new(io::Error::other("unexpected XDG system config path")));
    }

    Ok(())
}

#[test]
fn discover_threadplane_config_prefers_explicit_path() -> Result<(), Box<dyn Error>> {
    let explicit_path = PathBuf::from("/tmp/threadplane-explicit.toml");
    let discovery = discover_threadplane_config(Some(explicit_path.as_path()))?;

    if discovery.explicit_override != Some(explicit_path.clone()) {
        return Err(Box::new(io::Error::other("unexpected explicit_override")));
    }
    if discovery.selected_path != Some(explicit_path.clone()) {
        return Err(Box::new(io::Error::other("unexpected selected_path")));
    }
    if discovery.search_order != vec![explicit_path] {
        return Err(Box::new(io::Error::other("unexpected search_order")));
    }
    if discovery.env_prefix != ENV_PREFIX {
        return Err(Box::new(io::Error::other("unexpected env_prefix")));
    }

    Ok(())
}

#[test]
#[expect(
    clippy::result_large_err,
    reason = "figment::Jail fixes the closure error type to figment::Result."
)]
fn load_threadplane_config_with_path_reads_explicit_config() {
    Jail::expect_with(|jail| {
        jail.create_file("config.toml", full_config_body())?;
        let config_path = jail.directory().join("config.toml");
        let loaded = load_threadplane_config_with_path(Some(config_path.as_path()))
            .map_err(|error| error.to_string())?;

        assert_eq!(loaded.config.cli.url, "http://127.0.0.1:4123");
        assert_eq!(loaded.config.server.bind, "127.0.0.1:4321");
        assert_eq!(
            loaded.config.server.database_url,
            "postgres://threadplane:secret@127.0.0.1:5432/threadplane"
        );
        assert_eq!(loaded.config.server.default_lease_seconds, 42);
        assert_eq!(loaded.discovery.selected_path, Some(config_path));

        Ok(())
    });
}

#[test]
#[expect(
    clippy::result_large_err,
    reason = "figment::Jail fixes the closure error type to figment::Result."
)]
fn load_threadplane_config_with_overrides_applies_sparse_runtime_layer() {
    Jail::expect_with(|jail| {
        jail.create_file("config.toml", full_config_body())?;
        let config_path = jail.directory().join("config.toml");
        let overrides = ThreadplaneConfigOverrides {
            cli: Some(CliConfigOverrides {
                url: Some("http://127.0.0.1:4999".to_owned()),
            }),
            ..ThreadplaneConfigOverrides::default()
        };
        let loaded =
            load_threadplane_config_with_overrides(Some(config_path.as_path()), &overrides)
                .map_err(|error| error.to_string())?;

        assert_eq!(loaded.config.cli.url, "http://127.0.0.1:4999");
        assert_eq!(loaded.config.server.bind, "127.0.0.1:4321");

        Ok(())
    });
}

#[test]
#[expect(
    clippy::result_large_err,
    reason = "figment::Jail fixes the closure error type to figment::Result."
)]
fn load_threadplane_config_requires_all_fields_to_be_explicit() {
    let config_body = r#"
[cli]
url = "http://127.0.0.1:4123"

[server]
bind = "127.0.0.1:4321"
"#;
    Jail::expect_with(|jail| {
        jail.create_file("config.toml", config_body)?;
        let config_path = jail.directory().join("config.toml");

        let load_result = load_threadplane_config_with_path(Some(config_path.as_path()));
        assert!(load_result.is_err(), "incomplete config unexpectedly loaded");
        let rendered = load_result.err().map(|error| error.to_string()).unwrap_or_default();
        assert!(rendered.contains("configuration load failed"));

        Ok(())
    });
}

#[rstest]
#[case(vec![" Workflow ".to_owned(), "agent".to_owned(), "workflow".to_owned()], vec!["agent".to_owned(), "workflow".to_owned()])]
#[case(vec![String::new(), "   ".to_owned()], Vec::<String>::new())]
fn normalize_task_labels_sorts_dedups_and_trims(
    #[case] input: Vec<String>,
    #[case] expected: Vec<String>,
) {
    assert_eq!(normalize_task_labels(input), expected);
}

#[rstest]
#[case(Some(" codex ".to_owned()), Some("codex".to_owned()))]
#[case(Some("   ".to_owned()), None)]
#[case(None, None)]
fn normalize_task_owner_trims_and_discards_empty_values(
    #[case] input: Option<String>,
    #[case] expected: Option<String>,
) {
    assert_eq!(normalize_task_owner(input), expected);
}

#[rstest]
#[case("low", "low")]
#[case("medium", "medium")]
#[case("high", "high")]
#[case("urgent", "urgent")]
#[case("Urgent Fix", "urgent_fix")]
fn task_priority_parses_and_normalizes_values(#[case] input: &str, #[case] expected: &str) {
    assert_eq!(
        input.parse::<TaskPriority>().ok().map(|priority| priority.to_string()),
        Some(expected.to_owned())
    );
}

#[rstest]
#[case(WorkspaceRole::Viewer, true, false, false)]
#[case(WorkspaceRole::Editor, true, true, false)]
#[case(WorkspaceRole::Admin, true, true, true)]
fn workspace_role_capabilities_are_ordered(
    #[case] role: WorkspaceRole,
    #[case] can_view: bool,
    #[case] can_edit: bool,
    #[case] can_administer: bool,
) {
    assert_eq!(role.can_view(), can_view);
    assert_eq!(role.can_edit(), can_edit);
    assert_eq!(role.can_administer(), can_administer);
}

#[rstest]
#[case("Urgent Fix", "urgent_fix")]
#[case("expedite-now", "expedite_now")]
#[case("  customer blocker  ", "customer_blocker")]
fn normalize_workspace_priority_name_examples(
    #[case] input: &str,
    #[case] expected: &str,
) {
    assert_eq!(normalize_workspace_priority_name(input), expected);
}

#[test]
fn validate_workspace_priority_policy_accepts_unique_ranked_priorities() {
    let policy = sample_workspace_priority_policy();

    assert_eq!(validate_workspace_priority_policy(&policy), Ok(()));
}

#[test]
fn validate_workspace_priority_policy_rejects_duplicate_names_after_normalization() {
    let policy = WorkspacePriorityPolicy {
        default_priority: "urgent_fix".to_owned(),
        priorities: vec![
            WorkspacePriority {
                name: "Urgent Fix".to_owned(),
                rank: 10,
                description: None,
            },
            WorkspacePriority {
                name: "urgent_fix".to_owned(),
                rank: 20,
                description: None,
            },
        ],
    };

    let rendered = validate_workspace_priority_policy(&policy)
        .err()
        .map(|error| error.to_string());
    assert_eq!(
        rendered.as_deref(),
        Some("workspace priorities must use unique normalized names; duplicate `urgent_fix`")
    );
}

#[test]
fn validate_workspace_priority_policy_rejects_missing_default_priority() {
    let policy = WorkspacePriorityPolicy {
        default_priority: "normal".to_owned(),
        priorities: vec![WorkspacePriority {
            name: "expedite".to_owned(),
            rank: 10,
            description: None,
        }],
    };

    let rendered = validate_workspace_priority_policy(&policy)
        .err()
        .map(|error| error.to_string());
    assert_eq!(
        rendered.as_deref(),
        Some("workspace priorities must include the default priority `normal`")
    );
}

#[test]
fn validate_workspace_auth_policy_rejects_empty_algorithm_lists() {
    let rendered = validate_workspace_auth_policy(&WorkspaceAuthPolicy {
        allowed_algorithms: Vec::new(),
        challenge_ttl_seconds: 60,
        signed_commands_required: true,
    })
    .err()
    .map(|error| error.to_string());

    assert_eq!(
        rendered.as_deref(),
        Some("workspace auth policy must support at least one public-key algorithm")
    );
}

#[test]
fn validate_workspace_policy_accepts_governed_workspace_shape() {
    let policy = WorkspacePolicy {
        auth: WorkspaceAuthPolicy {
            allowed_algorithms: vec![
                PublicKeyAlgorithm::SshEd25519,
                PublicKeyAlgorithm::Ed25519,
            ],
            challenge_ttl_seconds: 90,
            signed_commands_required: true,
        },
        priorities: sample_workspace_priority_policy(),
        workspace: "shared-lab".to_owned(),
    };

    assert_eq!(validate_workspace_policy(&policy), Ok(()));
}

#[test]
fn workspace_priority_policy_exposes_default_and_support_checks() {
    let policy = sample_workspace_priority_policy();
    let default_priority = policy.default_task_priority().map(|value| value.to_string());
    let expedite_priority = TaskPriority::new("expedite");
    let background_priority = TaskPriority::new("background");

    assert_eq!(default_priority.as_deref(), Some("normal"));
    assert_eq!(expedite_priority.as_ref().map(|value| policy.supports(value)), Some(true));
    assert_eq!(background_priority.as_ref().and_then(|value| policy.rank_for(value)), Some(10));
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
