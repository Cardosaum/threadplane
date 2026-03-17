#![expect(
    clippy::redundant_pub_crate,
    reason = "Benchmark error types are only shared across crate-local modules."
)]

use core::result::Result as CoreResult;

use snafu::Snafu;

use threadplane_core::ThreadplaneError;

pub(crate) type Result<T, E = BenchError> = CoreResult<T, E>;

#[derive(Debug, Snafu)]
#[snafu(visibility(pub(crate)))]
#[snafu(context(suffix(false)))]
pub(crate) enum BenchError {
    #[snafu(display("benchmark configuration is invalid: {message}"))]
    Config {
        message: String,
        #[snafu(implicit)]
        location: snafu::Location,
    },

    #[snafu(display("failed to load threadplane config: {source}"))]
    ConfigLoad {
        source: ThreadplaneError,
        #[snafu(implicit)]
        location: snafu::Location,
    },

    #[snafu(display("failed to construct benchmark HTTP client: {source}"))]
    HttpClientBuild {
        source: reqwest::Error,
        #[snafu(implicit)]
        location: snafu::Location,
    },

    #[snafu(display("failed to render benchmark report as JSON: {source}"))]
    JsonRender {
        source: serde_json::Error,
        #[snafu(implicit)]
        location: snafu::Location,
    },
}
