use axum::{
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use tracing::error;

use super::ThreadplaneServerError;

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
