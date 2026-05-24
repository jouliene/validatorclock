use super::{chain_exists, unknown_chain_response};
use crate::chain::get_chain_snapshot_cached_first;
use crate::server::responses::json_error;
use crate::state::AppState;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::error;

pub(in crate::server) async fn chain_clock(
    State(state): State<Arc<AppState>>,
    Path(chain_id): Path<String>,
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    if !chain_exists(&state, &chain_id) {
        return unknown_chain_response(&chain_id);
    }

    let force_refresh = state.config.security.allow_force_refresh && query_forces_refresh(&query);
    match get_chain_snapshot_cached_first(Arc::clone(&state), &chain_id, force_refresh).await {
        Ok(snapshot) => Json(snapshot).into_response(),
        Err(error) => {
            error!(chain_id, error = ?error, "snapshot request failed");
            json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "chain_snapshot_failed",
                "failed to fetch chain snapshot",
            )
        }
    }
}

fn query_forces_refresh(query: &HashMap<String, String>) -> bool {
    query
        .get("refresh")
        .is_some_and(|value| matches!(value.as_str(), "1" | "true" | "yes"))
}
