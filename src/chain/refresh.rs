use super::util::now_sec;
use super::validator_sources::{
    apply_cached_validator_contract_type_hashes, update_validator_contract_type_hashes,
};
use super::{ClockSnapshot, fetch_chain_snapshot};
use crate::config::ChainConfig;
use crate::state::AppState;
use anyhow::{Result, anyhow};
use std::sync::Arc;
use tokio::time::{Duration, timeout};
use tracing::warn;

mod scheduler;

pub(crate) use scheduler::spawn_background_refresh;
use scheduler::spawn_stale_snapshot_refresh;

const VALIDATOR_TYPE_UPDATE_MIN_TIMEOUT_SECS: u64 = 5;
const VALIDATOR_TYPE_UPDATE_MAX_TIMEOUT_SECS: u64 = 30;
const STALE_REFRESH_WARNING: &str = "refresh is running in background";

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
    state
        .annotate_map_fake_validators(&mut snapshot, observed_at)
        .await;
    let cached_snapshot = state.cached_snapshot(chain_id).await;
    if let Some(reason) = cached_snapshot
        .as_ref()
        .and_then(|cached| degraded_refresh_reason(&snapshot, cached))
    {
        state
            .record_refresh_failure(chain_id, fetched_at, reason.clone())
            .await;
        if let Some(mut cached) = cached_snapshot {
            cached.warning = Some(degraded_refresh_cache_warning(cached.fetched_at, &reason));
            return cached;
        }
    }
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

fn degraded_refresh_cache_warning(fetched_at: u64, reason: &str) -> String {
    format!("using cached data from {fetched_at}; refresh returned degraded data: {reason}")
}

fn degraded_refresh_reason(refreshed: &ClockSnapshot, cached: &ClockSnapshot) -> Option<String> {
    if refreshed.current_set.round_id != cached.current_set.round_id
        || refreshed.current_set.utime_since != cached.current_set.utime_since
    {
        return None;
    }

    if refreshed.current_set.validators.is_empty() && !cached.current_set.validators.is_empty() {
        return Some("active validator set is empty".to_owned());
    }

    let refreshed_round_data = active_round_data_stats(refreshed);
    let cached_round_data = active_round_data_stats(cached);
    if cached_round_data.is_richer_than_empty()
        && (refreshed_round_data.is_empty()
            || cached_round_data.validator_details_dropped_to_zero(refreshed_round_data))
    {
        return Some("active validator round data is missing".to_owned());
    }

    if election_window_contains(refreshed)
        && refreshed.election.candidates.is_empty()
        && !cached.election.candidates.is_empty()
    {
        return Some("election candidates are missing during the election window".to_owned());
    }

    None
}

#[derive(Clone, Copy)]
struct ActiveRoundDataStats {
    total_fields: usize,
    validator_wallets: usize,
    validator_stakes: usize,
}

impl ActiveRoundDataStats {
    fn is_empty(self) -> bool {
        self.total_fields == 0 && self.validator_wallets == 0 && self.validator_stakes == 0
    }

    fn is_richer_than_empty(self) -> bool {
        !self.is_empty()
    }

    fn validator_details_dropped_to_zero(self, refreshed: Self) -> bool {
        (self.validator_wallets > 0 && refreshed.validator_wallets == 0)
            || (self.validator_stakes > 0 && refreshed.validator_stakes == 0)
    }
}

fn active_round_data_stats(snapshot: &ClockSnapshot) -> ActiveRoundDataStats {
    ActiveRoundDataStats {
        total_fields: usize::from(snapshot.current_set.total_stake.is_some())
            + usize::from(snapshot.current_set.total_reward.is_some()),
        validator_wallets: snapshot
            .current_set
            .validators
            .iter()
            .filter(|validator| validator.wallet.is_some())
            .count(),
        validator_stakes: snapshot
            .current_set
            .validators
            .iter()
            .filter(|validator| validator.stake.is_some())
            .count(),
    }
}

fn election_window_contains(snapshot: &ClockSnapshot) -> bool {
    let anchor = snapshot
        .next_set
        .as_ref()
        .map_or(snapshot.current_set.utime_until, |set| set.utime_until);
    let start =
        u64::from(anchor).saturating_sub(u64::from(snapshot.params15.elections_start_before));
    let end = u64::from(anchor).saturating_sub(u64::from(snapshot.params15.elections_end_before));

    snapshot.fetched_at >= start && snapshot.fetched_at < end
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
