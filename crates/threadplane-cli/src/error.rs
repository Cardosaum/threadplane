#![expect(
    clippy::redundant_pub_crate,
    reason = "CLI error types are exposed across crate-local modules only."
)]

use core::result::Result as CoreResult;

use derive_more::Display;
use snafu::Snafu;

use threadplane_core::ThreadplaneError;

pub(crate) type Result<T, E = CliError> = CoreResult<T, E>;

#[derive(Debug, Display)]
#[display(
    "CLI {} ({}) vs server {} ({}); changed fields: {}",
    cli_version,
    cli_commit,
    server_version,
    server_commit,
    changed_fields
)]
pub(crate) struct ContractMismatchDetails {
    pub(crate) changed_fields: String,
    pub(crate) cli_commit: String,
    pub(crate) cli_version: String,
    pub(crate) server_commit: String,
    pub(crate) server_version: String,
}

#[derive(Debug, Snafu)]
#[snafu(visibility(pub(crate)))]
#[snafu(context(suffix(false)))]
pub(crate) enum CliError {
    #[snafu(display("failed to load threadplane config: {source}"))]
    ConfigLoad {
        source: ThreadplaneError,
        #[snafu(implicit)]
        location: snafu::Location,
    },

    #[snafu(display("failed to construct HTTP client: {source}"))]
    HttpClientBuild {
        source: reqwest::Error,
        #[snafu(implicit)]
        location: snafu::Location,
    },

    #[snafu(display(
        "failed to parse JSON response from {url}: the running server appears to use a different contract than this CLI. {details}. Run `threadplane build compare` and restart the server.\noriginal parse error: {source}"
    ))]
    JsonContractMismatch {
        details: Box<ContractMismatchDetails>,
        url: String,
        source: serde_json::Error,
        #[snafu(implicit)]
        location: snafu::Location,
    },

    #[snafu(display("failed to parse JSON response from {url}: {source}"))]
    JsonParse {
        url: String,
        source: serde_json::Error,
        #[snafu(implicit)]
        location: snafu::Location,
    },

    #[snafu(display("failed to render JSON output: {source}"))]
    JsonRender {
        source: serde_json::Error,
        #[snafu(implicit)]
        location: snafu::Location,
    },

    #[snafu(display("{method} {url} returned {status}: {body}"))]
    NonSuccessStatus {
        method: &'static str,
        url: String,
        status: reqwest::StatusCode,
        body: String,
        #[snafu(implicit)]
        location: snafu::Location,
    },

    #[snafu(display("{method} {url} failed: {source}"))]
    RequestSend {
        method: &'static str,
        url: String,
        source: reqwest::Error,
        #[snafu(implicit)]
        location: snafu::Location,
    },

    #[snafu(display("failed to read response body from {url}: {source}"))]
    ResponseBodyRead {
        url: String,
        source: reqwest::Error,
        #[snafu(implicit)]
        location: snafu::Location,
    },

    #[snafu(display("{message}"))]
    Usage {
        message: String,
        #[snafu(implicit)]
        location: snafu::Location,
    },
}
