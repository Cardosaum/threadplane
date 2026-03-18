use super::*;

#[test]
fn current_build_info_reports_compiled_server_identity() {
    let build = current_build_info();
    let expected_dirty = matches!(env!("THREADPLANE_GIT_DIRTY"), "true");

    assert_eq!(build.service, "threadplane-server");
    assert_eq!(build.version, env!("CARGO_PKG_VERSION"));
    assert_eq!(build.build_profile, env!("THREADPLANE_BUILD_PROFILE"));
    assert_eq!(build.git_dirty, expected_dirty);
    assert!(!build.build_profile.is_empty());
}
