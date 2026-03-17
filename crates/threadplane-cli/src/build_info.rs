#![expect(
    clippy::redundant_pub_crate,
    reason = "Build info is shared across crate-local CLI modules."
)]

use threadplane_core::{build_info, BuildInfo};

const BUILD_PROFILE: &str = env!("THREADPLANE_BUILD_PROFILE");
const GIT_COMMIT: Option<&str> = option_env!("THREADPLANE_GIT_COMMIT");
const GIT_DIRTY_RAW: &str = env!("THREADPLANE_GIT_DIRTY");
const SERVICE: &str = env!("CARGO_PKG_NAME");
const VERSION: &str = env!("CARGO_PKG_VERSION");

#[must_use]
pub(crate) fn current_build_info() -> BuildInfo {
    build_info(
        SERVICE,
        VERSION,
        BUILD_PROFILE,
        GIT_COMMIT,
        matches!(GIT_DIRTY_RAW, "true"),
    )
}
