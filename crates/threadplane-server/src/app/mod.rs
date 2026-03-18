#![expect(
    clippy::arbitrary_source_item_ordering,
    reason = "App bootstrap groups lifecycle and wiring by layer rather than alphabetically."
)]
#![expect(
    clippy::redundant_pub_crate,
    reason = "Server app types are shared only across crate-local modules."
)]

mod config;
mod routes;
mod runtime;
mod state;

use self::config::AppConfig;
pub(crate) use self::config::WorkspaceGovernanceBootstrap;
use self::routes::build_router;
pub(crate) use self::runtime::run;
pub(crate) use self::state::{AppDependencies, AppState, LeasePolicy, ProjectionCoordinator};
