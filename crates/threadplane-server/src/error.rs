#![expect(
    clippy::arbitrary_source_item_ordering,
    reason = "Error variants are grouped by runtime layer for readability."
)]
#![expect(
    clippy::redundant_pub_crate,
    reason = "Server error types are shared only across crate-local modules."
)]

use core::{
    fmt::Display,
    net::{AddrParseError, SocketAddr},
    result::Result as CoreResult,
};
use std::io;

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use snafu::{IntoError as _, Location, Snafu};
use tracing::error;

pub(crate) type ServerResult<T, E = ThreadplaneServerError> = CoreResult<T, E>;
pub(crate) type AppResult<T> = ServerResult<Json<threadplane_core::ApiEnvelope<T>>>;

#[derive(Debug, Snafu)]
#[snafu(visibility(pub(crate)), context(suffix(false)))]
pub(crate) enum ThreadplaneServerError {
    #[snafu(display("failed to load threadplane config"))]
    LoadConfig {
        source: threadplane_core::ThreadplaneError,
        #[snafu(implicit)]
        location: Location,
    },

    #[snafu(display("invalid server.bind value: {value}"))]
    InvalidBindAddress {
        value: String,
        source: AddrParseError,
        #[snafu(implicit)]
        location: Location,
    },

    #[snafu(display("missing required configuration value {key}"))]
    MissingConfig {
        key: String,
        #[snafu(implicit)]
        location: Location,
    },

    #[snafu(display("failed to bind threadplane server to {bind_addr}"))]
    BindListener {
        bind_addr: SocketAddr,
        source: io::Error,
        #[snafu(implicit)]
        location: Location,
    },

    #[snafu(display("failed to connect to postgres at {database_url}"))]
    ConnectPostgres {
        database_url: String,
        source: sqlx::Error,
        #[snafu(implicit)]
        location: Location,
    },

    #[snafu(display("failed to connect to neo4j at {neo4j_uri}"))]
    ConnectNeo4j {
        neo4j_uri: String,
        source: neo4rs::Error,
        #[snafu(implicit)]
        location: Location,
    },

    #[snafu(display("failed to verify neo4j connectivity"))]
    VerifyNeo4j {
        source: neo4rs::Error,
        #[snafu(implicit)]
        location: Location,
    },

    #[snafu(display("threadplane server exited unexpectedly"))]
    Serve {
        source: io::Error,
        #[snafu(implicit)]
        location: Location,
    },

    #[snafu(display("database operation failed"))]
    Database {
        source: sqlx::Error,
        #[snafu(implicit)]
        location: Location,
    },

    #[snafu(display("graph operation failed"))]
    GraphOperation {
        source: neo4rs::Error,
        #[snafu(implicit)]
        location: Location,
    },

    #[snafu(display("json serialization failed"))]
    JsonSerialization {
        source: serde_json::Error,
        #[snafu(implicit)]
        location: Location,
    },

    #[snafu(display("graph row decoding failed"))]
    GraphDecode {
        source: neo4rs::DeError,
        #[snafu(implicit)]
        location: Location,
    },

    #[snafu(display("bad request: {msg}"))]
    BadRequest {
        msg: String,
        #[snafu(implicit)]
        location: Location,
    },

    #[snafu(display("conflict: {msg}"))]
    Conflict {
        msg: String,
        #[snafu(implicit)]
        location: Location,
    },

    #[snafu(display("not found: {msg}"))]
    NotFound {
        msg: String,
        #[snafu(implicit)]
        location: Location,
    },

    #[snafu(display("internal error: {msg}"))]
    Internal {
        msg: String,
        #[snafu(implicit)]
        location: Location,
    },
}

impl ThreadplaneServerError {
    pub(crate) fn bad_request(msg: impl Into<String>) -> Self {
        BadRequest { msg: msg.into() }.build()
    }

    pub(crate) fn conflict(msg: impl Into<String>) -> Self {
        Conflict { msg: msg.into() }.build()
    }

    pub(crate) fn not_found(msg: impl Into<String>) -> Self {
        NotFound { msg: msg.into() }.build()
    }

    pub(crate) fn internal(msg: impl Display) -> Self {
        Internal {
            msg: msg.to_string(),
        }
        .build()
    }

    const fn location(&self) -> &Location {
        match self {
            Self::LoadConfig { location, .. }
            | Self::InvalidBindAddress { location, .. }
            | Self::MissingConfig { location, .. }
            | Self::BindListener { location, .. }
            | Self::ConnectPostgres { location, .. }
            | Self::ConnectNeo4j { location, .. }
            | Self::VerifyNeo4j { location, .. }
            | Self::Serve { location, .. }
            | Self::Database { location, .. }
            | Self::GraphOperation { location, .. }
            | Self::JsonSerialization { location, .. }
            | Self::GraphDecode { location, .. }
            | Self::BadRequest { location, .. }
            | Self::Conflict { location, .. }
            | Self::NotFound { location, .. }
            | Self::Internal { location, .. } => location,
        }
    }

    const fn status_code(&self) -> StatusCode {
        match self {
            Self::BadRequest { .. } => StatusCode::BAD_REQUEST,
            Self::Conflict { .. } => StatusCode::CONFLICT,
            Self::NotFound { .. } => StatusCode::NOT_FOUND,
            Self::LoadConfig { .. }
            | Self::InvalidBindAddress { .. }
            | Self::MissingConfig { .. }
            | Self::BindListener { .. }
            | Self::ConnectPostgres { .. }
            | Self::ConnectNeo4j { .. }
            | Self::VerifyNeo4j { .. }
            | Self::Serve { .. }
            | Self::Database { .. }
            | Self::GraphOperation { .. }
            | Self::JsonSerialization { .. }
            | Self::GraphDecode { .. }
            | Self::Internal { .. } => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn response_message(&self) -> String {
        match self {
            Self::Database { source, .. } => source.to_string(),
            Self::GraphOperation { source, .. } => source.to_string(),
            Self::JsonSerialization { source, .. } => source.to_string(),
            Self::GraphDecode { source, .. } => source.to_string(),
            Self::LoadConfig { .. }
            | Self::InvalidBindAddress { .. }
            | Self::MissingConfig { .. }
            | Self::BindListener { .. }
            | Self::ConnectPostgres { .. }
            | Self::ConnectNeo4j { .. }
            | Self::VerifyNeo4j { .. }
            | Self::Serve { .. } => self.to_string(),
            Self::BadRequest { msg, .. }
            | Self::Conflict { msg, .. }
            | Self::NotFound { msg, .. }
            | Self::Internal { msg, .. } => msg.clone(),
        }
    }
}

impl IntoResponse for ThreadplaneServerError {
    fn into_response(self) -> Response {
        error!(error = %self, location = %self.location(), "request failed");
        (
            self.status_code(),
            Json(json!({
                "ok": false,
                "error": self.response_message(),
            })),
        )
            .into_response()
    }
}

impl From<sqlx::Error> for ThreadplaneServerError {
    fn from(value: sqlx::Error) -> Self {
        Database.into_error(value)
    }
}

impl From<neo4rs::Error> for ThreadplaneServerError {
    fn from(value: neo4rs::Error) -> Self {
        GraphOperation.into_error(value)
    }
}

impl From<serde_json::Error> for ThreadplaneServerError {
    fn from(value: serde_json::Error) -> Self {
        JsonSerialization.into_error(value)
    }
}

impl From<neo4rs::DeError> for ThreadplaneServerError {
    fn from(value: neo4rs::DeError) -> Self {
        GraphDecode.into_error(value)
    }
}
