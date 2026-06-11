use super::{chain_exists, unknown_chain_response};
use crate::chain::{chain_round_stats_from_history, fetch_chain_round_stats};
use crate::server::responses::json_error;
use crate::state::AppState;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::time::{Duration, timeout};
use tracing::error;

pub(in crate::server) async fn chain_round_stats(
    State(state): State<Arc<AppState>>,
    Path(chain_id): Path<String>,
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    if !chain_exists(&state, &chain_id) {
        return unknown_chain_response(&chain_id);
    }

    let chain = state
        .config
        .chain(&chain_id)
        .expect("chain exists after chain_exists")
        .clone();
    let history_points = state.round_stats_points(&chain_id).await;
    let cached_stats = state
        .cached_snapshot(&chain_id)
        .await
        .map(|snapshot| chain_round_stats_from_history(&snapshot, history_points.clone()));
    if query_prefers_cache(&query)
        && let Some(stats) = cached_stats.as_ref()
        && stats.has_round_data()
    {
        return Json(stats).into_response();
    }

    let timeout_seconds = state.config.refresh_timeout_seconds.min(8);
    match timeout(
        Duration::from_secs(timeout_seconds),
        fetch_chain_round_stats(&chain, history_points),
    )
    .await
    {
        Ok(Ok(stats)) => Json(stats).into_response(),
        Ok(Err(error)) => {
            error!(chain_id, error = ?error, "round stats request failed");
            if let Some(stats) = cached_stats {
                return Json(stats).into_response();
            }
            json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "chain_round_stats_failed",
                "failed to fetch chain round statistics",
            )
        }
        Err(_) => {
            error!(chain_id, timeout_seconds, "round stats request timed out");
            if let Some(stats) = cached_stats {
                return Json(stats).into_response();
            }
            json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "chain_round_stats_timeout",
                "chain round statistics request timed out",
            )
        }
    }
}

fn query_prefers_cache(query: &HashMap<String, String>) -> bool {
    query
        .get("prefer_cache")
        .is_some_and(|value| matches!(value.as_str(), "1" | "true" | "yes"))
}
