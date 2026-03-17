use core::error::Error;
use std::{
    env,
    fs,
    io,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use proptest::prelude::{any, Strategy};
use proptest::{prop_assert, prop_assert_eq};
use rstest::rstest;
use uuid::Uuid;

use crate::{
    build_info, compare_build_info, default_config_path, default_system_config_path,
    discover_threadplane_config, epic_entity_ref, load_threadplane_config_with_path,
    normalize_task_labels, normalize_task_owner, note_entity_ref, parse_entity_ref,
    relation_type, scope_summary, service_snapshot, task_entity_ref, EntityRef, EventKind,
    TaskPriority, ThreadplaneConfig, ENV_PREFIX,
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
fn threadplane_config_defaults_match_local_poc_expectations() {
    let config = ThreadplaneConfig::default();

    assert_eq!(config.cli.url, "http://127.0.0.1:4000");
    assert_eq!(config.server.bind, "127.0.0.1:4000");
    assert_eq!(config.server.default_lease_seconds, 300);
    assert_eq!(default_config_path().to_string_lossy(), "etc/config.toml");
    assert_eq!(
        default_system_config_path().to_string_lossy(),
        "/etc/threadplane/config.toml"
    );
    assert_eq!(config.server.database_url, None);
    assert_eq!(config.server.neo4j_password, None);
    assert_eq!(config.server.neo4j_uri, None);
    assert_eq!(config.server.neo4j_user, None);
}

#[test]
fn discover_threadplane_config_prefers_explicit_path() {
    let explicit_path = PathBuf::from("/tmp/threadplane-explicit.toml");
    let discovery = discover_threadplane_config(Some(explicit_path.as_path()));

    assert_eq!(discovery.explicit_override, Some(explicit_path.clone()));
    assert_eq!(discovery.selected_path, Some(explicit_path.clone()));
    assert_eq!(discovery.search_order, vec![explicit_path]);
    assert_eq!(discovery.env_prefix, ENV_PREFIX);
}

#[test]
fn load_threadplane_config_with_path_merges_toml_over_defaults() -> Result<(), Box<dyn Error>> {
    let config_dir = temp_config_dir();
    let config_path = config_dir.join("config.toml");
    let config_body = r#"
[cli]
url = "http://127.0.0.1:4123"

[server]
bind = "127.0.0.1:4321"
default_lease_seconds = 42
"#;
    fs::create_dir_all(&config_dir)?;
    fs::write(&config_path, config_body)?;

    let loaded = load_threadplane_config_with_path(Some(config_path.as_path()))?;

    if loaded.config.cli.url != "http://127.0.0.1:4123" {
        return Err(Box::new(io::Error::other("unexpected cli.url")));
    }
    if loaded.config.server.bind != "127.0.0.1:4321" {
        return Err(Box::new(io::Error::other("unexpected server.bind")));
    }
    if loaded.config.server.default_lease_seconds != 42 {
        return Err(Box::new(io::Error::other(
            "unexpected server.default_lease_seconds",
        )));
    }
    if loaded.discovery.selected_path != Some(config_path) {
        return Err(Box::new(io::Error::other("unexpected selected_path")));
    }

    fs::remove_dir_all(config_dir)?;
    Ok(())
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
#[case("low", TaskPriority::Low)]
#[case("medium", TaskPriority::Medium)]
#[case("high", TaskPriority::High)]
#[case("urgent", TaskPriority::Urgent)]
fn task_priority_parses_snake_case_values(#[case] input: &str, #[case] expected: TaskPriority) {
    assert_eq!(input.parse::<TaskPriority>().ok(), Some(expected));
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

fn temp_config_dir() -> PathBuf {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    env::temp_dir().join(format!("threadplane-core-config-{timestamp}"))
}
