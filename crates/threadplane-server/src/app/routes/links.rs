use axum::{routing::post, Router};

use crate::{
    app::AppState,
    handlers::{add_link, add_xanadu_link},
};

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route("/", post(add_link))
        .route("/xanadu", post(add_xanadu_link))
}
