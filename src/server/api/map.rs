use super::{chain_exists, unknown_chain_response};
use crate::chain::get_chain_snapshot_cached_first;
use crate::server::responses::{json_error, rendered_json};
use crate::state::AppState;
use crate::validator_map::{filter_map_nodes_to_validators, load_map_nodes};
use anyhow::Result;
use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::http::header::HeaderMap;
use axum::response::{IntoResponse, Response};
use serde_json::Value;
use std::sync::Arc;
use tracing::error;

pub(in crate::server) async fn chain_map(
    State(state): State<Arc<AppState>>,
    Path(chain_id): Path<String>,
    headers: HeaderMap,
) -> Response {
    if !chain_exists(&state, &chain_id) {
        return unknown_chain_response(&chain_id);
    }

    if let Some(rendered) = state.rendered_map(&chain_id).await {
        return rendered_json(&headers, &rendered);
    }

    // No map was written out with this chain's snapshot - it has none to show,
    // or nothing has been cached for it yet. Both are answered from here.
    match load_active_map_nodes(Arc::clone(&state), &chain_id).await {
        Ok(Some(nodes)) => Json(nodes).into_response(),
        Ok(None) => json_error(
            StatusCode::NOT_FOUND,
            "map_not_available",
            "validator map is not available for this chain",
        ),
        Err(error) => {
            error!(chain_id, error = ?error, "validator map request failed");
            json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "validator_map_failed",
                "failed to load validator map",
            )
        }
    }
}

/// The map file first: a chain with no map has none to show whether or not
/// its clock is running, and that is a plainer answer than the one a chain
/// with nothing cached would otherwise get.
async fn load_active_map_nodes(state: Arc<AppState>, chain_id: &str) -> Result<Option<Value>> {
    let Some(nodes) = load_map_nodes(&state.config, chain_id)? else {
        return Ok(None);
    };
    let snapshot = get_chain_snapshot_cached_first(state, chain_id, false).await?;
    filter_map_nodes_to_validators(nodes, &snapshot.current_set.validators).map(Some)
}
