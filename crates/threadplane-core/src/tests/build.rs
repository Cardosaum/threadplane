use super::*;

#[rstest]
#[case(EventKind::FactPromoted)]
#[case(EventKind::LinkDeclared)]
#[case(EventKind::EpicRecorded)]
#[case(EventKind::MemoryRecorded)]
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
    let build_object = scope.get("build").and_then(serde_json::Value::as_object);

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
    assert_eq!(
        fields,
        vec!["version", "build_profile", "git_commit", "git_dirty"]
    );
}
