use super::util::now_sec;
use super::validator_sources::{
    apply_cached_validator_contract_type_hashes, update_validator_contract_type_hashes,
};
use super::{ClockSnapshot, fetch_chain_snapshot};
use crate::config::ChainConfig;
use crate::state::AppState;
use anyhow::{Result, anyhow};
use std::sync::Arc;
use std::time::Instant;
use tokio::task::JoinSet;
use tokio::time::{Duration, MissedTickBehavior, interval, timeout};
use tracing::{info, warn};

const BACKGROUND_REFRESH_CONCURRENCY: usize = 2;
const VALIDATOR_TYPE_UPDATE_MIN_TIMEOUT_SECS: u64 = 5;
const VALIDATOR_TYPE_UPDATE_MAX_TIMEOUT_SECS: u64 = 30;
const STALE_REFRESH_WARNING: &str = "refresh is running in background";

#[derive(Clone, Copy)]
enum RefreshLogKind {
    Background,
    StaleCache,
}

impl RefreshLogKind {
    fn label(self) -> &'static str {
        match self {
            Self::Background => "background",
            Self::StaleCache => "stale_cache",
        }
    }
}

pub(crate) fn spawn_background_refresh(state: Arc<AppState>) {
    tokio::spawn(async move {
        background_refresh_loop(state).await;
    });
}

async fn background_refresh_loop(state: Arc<AppState>) {
    let refresh_seconds = state.config.refresh_seconds.max(10);
    info!(refresh_seconds, "background chain refresh started");
    let mut ticker = interval(Duration::from_secs(refresh_seconds));
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

    loop {
        ticker.tick().await;
        refresh_configured_chains(Arc::clone(&state)).await;
    }
}

async fn refresh_configured_chains(state: Arc<AppState>) {
    let mut chain_ids = state
        .config
        .chains
        .iter()
        .map(|chain| chain.id.clone())
        .collect::<Vec<_>>()
        .into_iter();
    let mut tasks = JoinSet::new();

    loop {
        while tasks.len() < BACKGROUND_REFRESH_CONCURRENCY {
            let Some(chain_id) = chain_ids.next() else {
                break;
            };
            let task_state = Arc::clone(&state);
            tasks.spawn(async move {
                refresh_chain_and_log(&task_state, &chain_id, RefreshLogKind::Background).await;
            });
        }

        if tasks.is_empty() {
            break;
        }

        if let Some(result) = tasks.join_next().await
            && let Err(error) = result
        {
            warn!(
                error = ?error,
                "background refresh task failed"
            );
        }
    }
}

async fn refresh_chain_and_log(state: &AppState, chain_id: &str, log_kind: RefreshLogKind) {
    let refresh_kind = log_kind.label();
    let started_at = Instant::now();
    match get_chain_snapshot(state, chain_id, true).await {
        Ok(snapshot) if snapshot.warning.is_some() => {
            info!(
                refresh_kind,
                chain_id,
                duration_ms = started_at.elapsed().as_millis(),
                fetched_at = snapshot.fetched_at,
                round_id = snapshot.current_set.round_id,
                round_color = ?snapshot.current_set.round_color,
                warning = ?snapshot.warning,
                "chain refresh completed with cached data"
            );
        }
        Ok(snapshot) => {
            info!(
                refresh_kind,
                chain_id,
                duration_ms = started_at.elapsed().as_millis(),
                fetched_at = snapshot.fetched_at,
                round_id = snapshot.current_set.round_id,
                round_color = ?snapshot.current_set.round_color,
                "chain refresh completed"
            );
        }
        Err(error) => {
            warn!(
                refresh_kind,
                chain_id,
                duration_ms = started_at.elapsed().as_millis(),
                error = ?error,
                "chain refresh failed"
            );
        }
    }
}

async fn get_chain_snapshot(
    state: &AppState,
    chain_id: &str,
    force_refresh: bool,
) -> Result<ClockSnapshot> {
    let now = now_sec()?;
    let refresh_seconds = state.config.refresh_seconds.max(10);

    if !force_refresh
        && let Some(snapshot) = state
            .cached_snapshot_if_fresh(chain_id, now, refresh_seconds)
            .await
    {
        return Ok(snapshot);
    }

    let chain = state
        .config
        .chain(chain_id)
        .ok_or_else(|| anyhow!("unknown chain id `{chain_id}`"))?;
    state.record_refresh_attempt(chain_id, now).await;

    match refresh_chain_with_timeout(state, chain).await {
        Ok(snapshot) => {
            Ok(finalize_refreshed_snapshot(state, chain, chain_id, now, snapshot).await)
        }
        Err(error) => cached_snapshot_after_refresh_failure(state, chain_id, now, error).await,
    }
}

async fn refresh_chain_with_timeout(
    state: &AppState,
    chain: &ChainConfig,
) -> Result<ClockSnapshot> {
    let timeout_seconds = state.config.refresh_timeout_seconds;
    timeout(
        Duration::from_secs(timeout_seconds),
        fetch_chain_snapshot_with_validator_types(state, chain),
    )
    .await
    .unwrap_or_else(|_| Err(anyhow!("refresh timed out after {timeout_seconds}s")))
}

async fn finalize_refreshed_snapshot(
    state: &AppState,
    chain: &ChainConfig,
    chain_id: &str,
    cache_checked_at: u64,
    mut snapshot: ClockSnapshot,
) -> ClockSnapshot {
    let fetched_at = snapshot.fetched_at;
    let observed_at = now_sec().unwrap_or(snapshot.fetched_at);
    state.annotate_map_fake_validators(&mut snapshot, observed_at);
    state.record_round_history(&mut snapshot, observed_at).await;
    apply_cached_validator_contract_type_hashes(state, chain, &mut snapshot).await;
    state
        .store_cached_snapshot(chain_id, cache_checked_at, snapshot.clone())
        .await;
    state.record_refresh_success(chain_id, fetched_at).await;
    snapshot
}

async fn cached_snapshot_after_refresh_failure(
    state: &AppState,
    chain_id: &str,
    failed_at: u64,
    error: anyhow::Error,
) -> Result<ClockSnapshot> {
    let error_message = error.to_string();
    state
        .record_refresh_failure(chain_id, failed_at, error_message)
        .await;
    if let Some(mut snapshot) = state.cached_snapshot(chain_id).await {
        snapshot.warning = Some(refresh_failed_cache_warning(snapshot.fetched_at, &error));
        return Ok(snapshot);
    }
    Err(error)
}

fn refresh_failed_cache_warning(fetched_at: u64, error: &anyhow::Error) -> String {
    format!("using cached data from {fetched_at}; refresh failed: {error}")
}

pub(crate) async fn get_chain_snapshot_cached_first(
    state: Arc<AppState>,
    chain_id: &str,
    force_refresh: bool,
) -> Result<ClockSnapshot> {
    let now = now_sec()?;
    let refresh_seconds = state.config.refresh_seconds.max(10);

    if !force_refresh {
        if let Some(snapshot) = state
            .cached_snapshot_if_fresh(chain_id, now, refresh_seconds)
            .await
        {
            return Ok(snapshot);
        }

        if let Some(mut snapshot) = state.cached_snapshot(chain_id).await {
            snapshot.warning = Some(stale_cache_refresh_warning(snapshot.fetched_at));
            spawn_stale_snapshot_refresh(Arc::clone(&state), chain_id.to_owned(), now).await;
            return Ok(snapshot);
        }
    }

    get_chain_snapshot(&state, chain_id, force_refresh).await
}

fn stale_cache_refresh_warning(fetched_at: u64) -> String {
    format!("using cached data from {fetched_at}; {STALE_REFRESH_WARNING}")
}

async fn spawn_stale_snapshot_refresh(state: Arc<AppState>, chain_id: String, now: u64) {
    let retry_after_seconds = state
        .config
        .refresh_seconds
        .max(10)
        .min(state.config.refresh_timeout_seconds.max(10));
    if !state
        .mark_refresh_attempt_if_due(&chain_id, now, retry_after_seconds)
        .await
    {
        return;
    }

    tokio::spawn(async move {
        refresh_chain_and_log(&state, &chain_id, RefreshLogKind::StaleCache).await;
    });
}

async fn fetch_chain_snapshot_with_validator_types(
    state: &AppState,
    chain: &ChainConfig,
) -> Result<ClockSnapshot> {
    let mut snapshot = fetch_chain_snapshot(chain).await?;
    let type_update_timeout =
        Duration::from_secs((state.config.refresh_timeout_seconds / 3).clamp(
            VALIDATOR_TYPE_UPDATE_MIN_TIMEOUT_SECS,
            VALIDATOR_TYPE_UPDATE_MAX_TIMEOUT_SECS,
        ));
    match timeout(
        type_update_timeout,
        update_validator_contract_type_hashes(state, chain, &mut snapshot),
    )
    .await
    {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            warn!(
                chain_id = %chain.id,
                error = ?error,
                "failed to update validator contract type hashes"
            );
        }
        Err(_) => {
            warn!(
                chain_id = %chain.id,
                timeout_seconds = type_update_timeout.as_secs(),
                "validator contract type hash update timed out"
            );
        }
    }
    Ok(snapshot)
}

#[cfg(test)]
mod tests;
