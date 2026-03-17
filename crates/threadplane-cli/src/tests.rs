use crate::command::build_mismatch_warning;
use threadplane_core::{build_info, compare_build_info};

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
