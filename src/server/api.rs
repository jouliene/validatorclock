use super::security::{json_error, query_forces_refresh};
use crate::chain::{chains_response, get_chain_snapshot_cached_first, runtime_status};
use crate::state::AppState;
use crate::tycho_map::{TYCHO_MAP_CHAIN_ID, filter_map_nodes_to_validators, load_tycho_map_nodes};
use anyhow::Result;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::error;

pub(super) async fn health() -> impl IntoResponse {
    Json(json!({ "status": "ok" }))
}

pub(super) async fn status(State(state): State<Arc<AppState>>) -> Response {
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

pub(super) async fn list_chains(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    Json(chains_response(&state.config))
}

pub(super) async fn chain_clock(
    State(state): State<Arc<AppState>>,
    Path(chain_id): Path<String>,
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    let force_refresh = state.config.security.allow_force_refresh && query_forces_refresh(&query);
    if state.config.chain(&chain_id).is_none() {
        return json_error(
            StatusCode::NOT_FOUND,
            "unknown_chain",
            &format!("unknown chain id `{chain_id}`"),
        );
    }

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

pub(super) async fn chain_map(
    State(state): State<Arc<AppState>>,
    Path(chain_id): Path<String>,
) -> Response {
    if state.config.chain(&chain_id).is_none() {
        return json_error(
            StatusCode::NOT_FOUND,
            "unknown_chain",
            &format!("unknown chain id `{chain_id}`"),
        );
    }
    if chain_id != TYCHO_MAP_CHAIN_ID {
        return json_error(
            StatusCode::NOT_FOUND,
            "map_not_available",
            "validator map is available for Tycho only",
        );
    }

    match load_active_tycho_map_nodes(Arc::clone(&state)).await {
        Ok(nodes) => Json(nodes).into_response(),
        Err(error) => {
            error!(error = ?error, "tycho map request failed");
            json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "tycho_map_failed",
                "failed to load Tycho validator map",
            )
        }
    }
}

async fn load_active_tycho_map_nodes(state: Arc<AppState>) -> Result<Value> {
    let nodes = load_tycho_map_nodes(&state.config)?;
    let snapshot = get_chain_snapshot_cached_first(state, TYCHO_MAP_CHAIN_ID, false).await?;
    filter_map_nodes_to_validators(nodes, &snapshot.current_set.validators)
}

pub(super) async fn not_found() -> Response {
    json_error(StatusCode::NOT_FOUND, "not_found", "not found")
}
