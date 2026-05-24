use crate::chain::chains_response;
use crate::state::AppState;
use axum::Json;
use axum::extract::State;
use axum::response::IntoResponse;
use std::sync::Arc;

pub(in crate::server) async fn list_chains(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    Json(chains_response(&state.config))
}
