use super::*;

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
#[case("Core Memory", "core_memory")]
#[case("workflow/policy", "workflow_policy")]
#[case("   ", "")]
fn normalize_memory_kind_name_examples(#[case] input: &str, #[case] expected: &str) {
    assert_eq!(normalize_memory_kind_name(input), expected);
}

#[rstest]
#[case(vec!["Prime".to_owned(), "core".to_owned(), " prime ".to_owned()], vec!["core".to_owned(), "prime".to_owned()])]
#[case(vec![String::new(), "  ".to_owned()], Vec::<String>::new())]
fn normalize_memory_tags_dedups_and_trims(
    #[case] input: Vec<String>,
    #[case] expected: Vec<String>,
) {
    assert_eq!(normalize_memory_tags(input), expected);
}

#[rstest]
#[case(vec!["Session Start".to_owned(), "pre_compact".to_owned(), "session-start".to_owned()], vec!["pre_compact".to_owned(), "session_start".to_owned()])]
#[case(vec![String::new()], Vec::<String>::new())]
fn normalize_memory_recall_triggers_normalize_identifier_lists(
    #[case] input: Vec<String>,
    #[case] expected: Vec<String>,
) {
    assert_eq!(normalize_memory_recall_triggers(input), expected);
}

#[rstest]
#[case("workflow_memory", Some("workflow_memory"))]
#[case("   ", None)]
fn memory_kind_parses_and_normalizes_values(#[case] input: &str, #[case] expected: Option<&str>) {
    assert_eq!(
        input
            .parse::<MemoryKind>()
            .ok()
            .map(|kind| kind.to_string()),
        expected.map(str::to_owned)
    );
}

#[rstest]
#[case(MemoryAudience::Agent, MemoryAudience::Agent, true)]
#[case(MemoryAudience::Both, MemoryAudience::Agent, true)]
#[case(MemoryAudience::Human, MemoryAudience::Agent, false)]
fn memory_audience_matches_requested_consumers(
    #[case] stored: MemoryAudience,
    #[case] requested: MemoryAudience,
    #[case] expected: bool,
) {
    assert_eq!(stored.includes(requested), expected);
}

#[rstest]
#[case(MemoryImportance::Normal, 10)]
#[case(MemoryImportance::High, 20)]
#[case(MemoryImportance::Critical, 30)]
fn memory_importance_exposes_stable_sort_ranks(
    #[case] importance: MemoryImportance,
    #[case] expected_rank: u8,
) {
    assert_eq!(importance.rank(), expected_rank);
}

#[rstest]
#[case(MemoryScope::Workspace, "workspace")]
#[case(MemoryScope::Repo, "repo")]
#[case(MemoryScope::Global, "global")]
fn memory_scope_serializes_to_snake_case(#[case] scope: MemoryScope, #[case] expected: &str) {
    assert_eq!(scope.to_string(), expected);
}

#[rstest]
#[case("low", "low")]
#[case("medium", "medium")]
#[case("high", "high")]
#[case("urgent", "urgent")]
#[case("Urgent Fix", "urgent_fix")]
fn task_priority_parses_and_normalizes_values(#[case] input: &str, #[case] expected: &str) {
    assert_eq!(
        input
            .parse::<TaskPriority>()
            .ok()
            .map(|priority| priority.to_string()),
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
fn normalize_workspace_priority_name_examples(#[case] input: &str, #[case] expected: &str) {
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
            allowed_algorithms: vec![PublicKeyAlgorithm::SshEd25519, PublicKeyAlgorithm::Ed25519],
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
    let default_priority = policy
        .default_task_priority()
        .map(|value| value.to_string());
    let expedite_priority = TaskPriority::new("expedite");
    let background_priority = TaskPriority::new("background");

    assert_eq!(default_priority.as_deref(), Some("normal"));
    assert_eq!(
        expedite_priority
            .as_ref()
            .map(|value| policy.supports(value)),
        Some(true)
    );
    assert_eq!(
        background_priority
            .as_ref()
            .and_then(|value| policy.rank_for(value)),
        Some(10)
    );
}
