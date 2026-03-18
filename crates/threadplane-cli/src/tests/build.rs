use super::*;

#[test]
fn build_mismatch_warning_lists_changed_fields() {
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

    let warning_message = build_mismatch_warning(&comparison);

    assert!(warning_message.is_some());
    let warning_text = warning_message.unwrap_or_default();
    assert!(warning_text.contains("changed fields: version, build_profile, git_commit, git_dirty"));
}

#[test]
fn build_mismatch_warning_is_absent_when_builds_match() {
    let client = build_info(
        "threadplane-cli",
        "0.1.0",
        "debug",
        Some("aaaaaaaaaaaa"),
        true,
    );
    let server = build_info(
        "threadplane-cli",
        "0.1.0",
        "debug",
        Some("aaaaaaaaaaaa"),
        true,
    );
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
        json_path: "data.labels".to_owned(),
        url: "http://127.0.0.1:4000/v1/workspaces/threadplane-dev/tasks".to_owned(),
    }
    .into_error(serde_json::Error::io(IoError::other(
        "missing field `labels`",
    )));

    let rendered = error.to_string();

    assert!(rendered.contains("different contract"));
    assert!(rendered.contains("Run `threadplane build compare`"));
    assert!(rendered.contains("0.1.0"));
    assert!(rendered.contains("0.2.0"));
}
