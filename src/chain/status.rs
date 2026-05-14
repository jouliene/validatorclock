use super::dto::ChainRuntimeStatusDto;
use super::{ChainMeta, ChainsResponse, RuntimeStatusResponse};
use crate::config::AppConfig;
use crate::state::AppState;
use anyhow::Result;

pub(crate) fn chains_response(config: &AppConfig) -> ChainsResponse {
    ChainsResponse {
        refresh_seconds: config.refresh_seconds,
        chains: config.chains.iter().map(ChainMeta::from).collect(),
    }
}

pub(crate) async fn runtime_status(state: &AppState) -> Result<RuntimeStatusResponse> {
    let now = super::util::now_sec()?;
    let refresh_seconds = state.config.refresh_seconds.max(10);
    let runtime_snapshots = state.chain_runtime_snapshots(now, refresh_seconds).await;
    let mut any_missing = false;
    let mut any_stale_error = false;

    let chains = state
        .config
        .chains
        .iter()
        .map(|chain| {
            let status = runtime_snapshots
                .get(&chain.id)
                .cloned()
                .unwrap_or_default();

            if !status.cached {
                any_missing = true;
            }
            if status.stale && status.last_error.is_some() {
                any_stale_error = true;
            }

            ChainRuntimeStatusDto {
                id: chain.id.clone(),
                name: chain.name.clone(),
                cached: status.cached,
                fetched_at: status.fetched_at,
                age_seconds: status.age_seconds,
                stale: status.stale,
                last_attempt_at: status.last_attempt_at,
                last_success_at: status.last_success_at,
                last_error: status.last_error,
            }
        })
        .collect();

    let status = if any_stale_error {
        "degraded"
    } else if any_missing {
        "starting"
    } else {
        "ok"
    };

    Ok(RuntimeStatusResponse {
        status,
        version: env!("CARGO_PKG_VERSION"),
        started_at: state.started_at_seconds(),
        uptime_seconds: state.uptime_seconds(),
        refresh_seconds,
        refresh_timeout_seconds: state.config.refresh_timeout_seconds,
        chains,
    })
}
