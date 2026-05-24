use crate::chain::runtime_status;
use crate::server::responses::json_error;
use crate::state::AppState;
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;
use std::sync::Arc;
use tracing::error;

pub(in crate::server) async fn health() -> impl IntoResponse {
    Json(json!({ "status": "ok" }))
}

pub(in crate::server) async fn status(State(state): State<Arc<AppState>>) -> Response {
    match runtime_status(&state).await {
        Ok(status) => Json(status).into_response(),
        Err(error) => {
            error!(error = ?error, "status request failed");
            json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "status_failed",
                "failed to build runtime status",
            )
        }
    }
}
