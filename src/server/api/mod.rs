use crate::server::responses::json_error;
use crate::state::AppState;
use axum::http::StatusCode;
use axum::response::Response;

mod chains;
mod clock;
mod map;
mod round_stats;
mod status;

pub(super) use chains::list_chains;
pub(super) use clock::chain_clock;
pub(super) use map::chain_map;
pub(super) use round_stats::chain_round_stats;
pub(super) use status::{health, status};

fn chain_exists(state: &AppState, chain_id: &str) -> bool {
    state.config.chain(chain_id).is_some()
}

fn unknown_chain_response(chain_id: &str) -> Response {
    json_error(
        StatusCode::NOT_FOUND,
        "unknown_chain",
        &format!("unknown chain id `{chain_id}`"),
    )
}
