#![expect(
    clippy::arbitrary_source_item_ordering,
    reason = "Error variants are grouped by runtime layer for readability."
)]
#![expect(
    clippy::redundant_pub_crate,
    reason = "Server error types are shared only across crate-local modules."
)]

mod http;
mod kinds;

pub(crate) use kinds::{AppResult, ServerResult, ThreadplaneServerError};
pub(crate) use kinds::{
    BindListener, ConnectNeo4j, ConnectPostgres, DatabaseMigration, InvalidBindAddress,
    InvalidWorkspaceBootstrap, LoadConfig, Serve, VerifyNeo4j,
};
