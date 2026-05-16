use super::assets::fallback_tycho_nodes_json;
use super::security::{json_error, query_forces_refresh};
use crate::chain::{chains_response, get_chain_snapshot_cached_first, runtime_status};
use crate::state::AppState;
use anyhow::{Context, Result, bail};
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::ErrorKind;
use std::sync::Arc;
use tracing::error;

const TYCHO_MAP_CHAIN_ID: &str = "tycho-testnet";

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

    match load_tycho_map_nodes(&state).await {
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

async fn load_tycho_map_nodes(state: &AppState) -> Result<Value> {
    if let Some(path) = &state.config.tycho_map_nodes_path {
        match std::fs::read_to_string(path) {
            Ok(body) => {
                let value = serde_json::from_str(&body)
                    .with_context(|| format!("failed to parse {}", path.display()))?;
                return ensure_map_nodes_array(value);
            }
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error).with_context(|| format!("failed to read {}", path.display()));
            }
        }
    }

    let value = fallback_tycho_nodes_json().context("failed to parse bundled Tycho map nodes")?;
    ensure_map_nodes_array(value)
}

fn ensure_map_nodes_array(value: Value) -> Result<Value> {
    if !value.is_array() {
        bail!("Tycho map nodes payload must be a JSON array");
    }
    Ok(value)
}

pub(super) async fn not_found() -> Response {
    json_error(StatusCode::NOT_FOUND, "not_found", "not found")
}
