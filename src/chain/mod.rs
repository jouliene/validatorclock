use crate::config::{AppConfig, ChainConfig};
use crate::history::save_round_history_merged;
use crate::state::AppState;
use anyhow::{Result, anyhow};
use std::sync::Arc;
use std::time::Instant;
use tokio::time::{Duration, MissedTickBehavior, interval, timeout};
use tracing::{info, warn};

const VALIDATOR_TYPE_UPDATE_MIN_TIMEOUT_SECS: u64 = 5;
const VALIDATOR_TYPE_UPDATE_MAX_TIMEOUT_SECS: u64 = 30;

mod dto;
mod elector;
mod util;
mod validator_sources;

use dto::ChainRuntimeStatusDto;
pub(crate) use dto::{
    CacheEntry, ChainMeta, ChainsResponse, ClockSnapshot, ElectionCandidateDto, ElectionDto,
    ElectionTimingsDto, RoundColor, RuntimeStatusResponse, ValidatorDto, ValidatorSetDto,
    ValidatorSourceDto,
};
pub(crate) use elector::fetch_chain_snapshot;
use util::now_sec;
use validator_sources::update_validator_contract_type_hashes;

#[cfg(test)]
pub(crate) fn test_cache_entry(fetched_at: u64, snapshot: ClockSnapshot) -> CacheEntry {
    CacheEntry {
        fetched_at,
        snapshot,
    }
}

#[cfg(test)]
pub(crate) fn test_clock_snapshot(chain_id: &str) -> ClockSnapshot {
    ClockSnapshot {
        chain: ChainMeta {
            id: chain_id.to_owned(),
            name: "Test".to_owned(),
            color: "#38bdf8".to_owned(),
            token_symbol: "TEST".to_owned(),
            rpc_label: "example.com".to_owned(),
        },
        fetched_at: 123,
        global_id: 42,
        seqno: 7,
        params15: ElectionTimingsDto {
            validators_elected_for: 65_536,
            elections_start_before: 32_768,
            elections_end_before: 8_192,
            stake_held_for: 32_768,
        },
        current_set: ValidatorSetDto {
            utime_since: 1000,
            utime_until: 2000,
            round_id: 10,
            round_color: RoundColor::Blue,
            total: 1,
            main: 1,
            total_weight: "1".to_owned(),
            total_stake: Some("100".to_owned()),
            total_reward: Some("1".to_owned()),
            validators: vec![ValidatorDto {
                public_key: "validator-key".to_owned(),
                adnl_addr: Some("adnl".to_owned()),
                wallet: Some("-1:wallet".to_owned()),
                source: None,
                contract_type: Some("EverWallet".to_owned()),
                contract_type_hash: Some(
                    "3ba6528ab2694c118180aa3bd10dd19ff400b909ab4dcf58fc69925b2c7b12a6".to_owned(),
                ),
                stake: Some("100".to_owned()),
                reward: Some("1".to_owned()),
                weight: "1".to_owned(),
                weight_percent: 100.0,
                history: Vec::new(),
            }],
            recent_absent_validators: Vec::new(),
        },
        previous_set: None,
        next_set: None,
        election: ElectionDto::default(),
        warning: None,
    }
}

pub(crate) fn chains_response(config: &AppConfig) -> ChainsResponse {
    ChainsResponse {
        refresh_seconds: config.refresh_seconds,
        chains: config.chains.iter().map(ChainMeta::from).collect(),
    }
}

pub(crate) async fn runtime_status(state: &AppState) -> Result<RuntimeStatusResponse> {
    let now = now_sec()?;
    let refresh_seconds = state.config.refresh_seconds.max(10);
    let cache = state.cache.read().await;
    let chain_status = state.chain_status.read().await;
    let mut any_missing = false;
    let mut any_stale_error = false;

    let chains = state
        .config
        .chains
        .iter()
        .map(|chain| {
            let cached = cache.get(&chain.id);
            let fetched_at = cached.map(|entry| entry.snapshot.fetched_at);
            let age_seconds = fetched_at.map(|fetched_at| now.saturating_sub(fetched_at));
            let stale = age_seconds.is_none_or(|age| age > refresh_seconds.saturating_mul(2));
            let status = chain_status.get(&chain.id);

            if cached.is_none() {
                any_missing = true;
            }
            if stale
                && status
                    .and_then(|status| status.last_error.as_ref())
                    .is_some()
            {
                any_stale_error = true;
            }

            ChainRuntimeStatusDto {
                id: chain.id.clone(),
                name: chain.name.clone(),
                cached: cached.is_some(),
                fetched_at,
                age_seconds,
                stale,
                last_attempt_at: status.and_then(|status| status.last_attempt_at),
                last_success_at: status.and_then(|status| status.last_success_at),
                last_error: status.and_then(|status| status.last_error.clone()),
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
        refresh_configured_chains(&state).await;
    }
}

async fn refresh_configured_chains(state: &AppState) {
    let chain_ids = state
        .config
        .chains
        .iter()
        .map(|chain| chain.id.clone())
        .collect::<Vec<_>>();

    for chain_id in chain_ids {
        let started_at = Instant::now();
        match get_chain_snapshot(state, &chain_id, true).await {
            Ok(snapshot) if snapshot.warning.is_some() => {
                info!(
                    chain_id,
                    duration_ms = started_at.elapsed().as_millis(),
                    fetched_at = snapshot.fetched_at,
                    round_id = snapshot.current_set.round_id,
                    round_color = ?snapshot.current_set.round_color,
                    warning = ?snapshot.warning,
                    "background refresh completed with cached data"
                );
            }
            Ok(snapshot) => {
                info!(
                    chain_id,
                    duration_ms = started_at.elapsed().as_millis(),
                    fetched_at = snapshot.fetched_at,
                    round_id = snapshot.current_set.round_id,
                    round_color = ?snapshot.current_set.round_color,
                    "background refresh completed"
                );
            }
            Err(error) => {
                warn!(
                    chain_id,
                    duration_ms = started_at.elapsed().as_millis(),
                    error = ?error,
                    "background refresh failed"
                );
            }
        }
    }
}

pub(crate) async fn get_chain_snapshot(
    state: &AppState,
    chain_id: &str,
    force_refresh: bool,
) -> Result<ClockSnapshot> {
    let now = now_sec()?;
    let refresh_seconds = state.config.refresh_seconds.max(10);

    if !force_refresh
        && let Some(entry) = state.cache.read().await.get(chain_id)
        && now.saturating_sub(entry.fetched_at) < refresh_seconds
    {
        let mut snapshot = entry.snapshot.clone();
        state
            .round_history
            .read()
            .await
            .annotate_snapshot(chain_id, &mut snapshot);
        return Ok(snapshot);
    }

    let chain = state
        .config
        .chain(chain_id)
        .ok_or_else(|| anyhow!("unknown chain id `{chain_id}`"))?;
    state.record_refresh_attempt(chain_id, now).await;

    let timeout_seconds = state.config.refresh_timeout_seconds;
    let refresh_result = timeout(
        Duration::from_secs(timeout_seconds),
        fetch_chain_snapshot_with_validator_types(state, chain),
    )
    .await
    .unwrap_or_else(|_| Err(anyhow!("refresh timed out after {timeout_seconds}s")));

    match refresh_result {
        Ok(mut snapshot) => {
            let fetched_at = snapshot.fetched_at;
            update_round_history(state, &mut snapshot).await;
            state.cache.write().await.insert(
                chain_id.to_owned(),
                CacheEntry {
                    fetched_at: now,
                    snapshot: snapshot.clone(),
                },
            );
            state.record_refresh_success(chain_id, fetched_at).await;
            Ok(snapshot)
        }
        Err(error) => {
            let error_message = error.to_string();
            state
                .record_refresh_failure(chain_id, now, error_message)
                .await;
            if let Some(entry) = state.cache.read().await.get(chain_id) {
                let mut snapshot = entry.snapshot.clone();
                state
                    .round_history
                    .read()
                    .await
                    .annotate_snapshot(chain_id, &mut snapshot);
                snapshot.warning = Some(format!(
                    "using cached data from {}; refresh failed: {error}",
                    snapshot.fetched_at
                ));
                return Ok(snapshot);
            }
            Err(error)
        }
    }
}

async fn update_round_history(state: &AppState, snapshot: &mut ClockSnapshot) {
    let chain_id = snapshot.chain.id.clone();
    let observed_at = now_sec().unwrap_or(snapshot.fetched_at);
    let retention = crate::history::RoundHistoryStore::retention_for_snapshot(&chain_id, snapshot);
    let history_path = state.round_history_path_for_chain(&chain_id);
    let history_to_save = {
        let mut history = state.round_history.write().await;
        let rounds_before = history.round_count_for_chain(&chain_id);
        let changed = history.record_snapshot(&chain_id, snapshot, observed_at);
        history.annotate_snapshot(&chain_id, snapshot);
        let rounds_after = history.round_count_for_chain(&chain_id);
        if changed || !history_path.exists() {
            info!(
                chain_id,
                path = %history_path.display(),
                rounds_before,
                rounds_after,
                changed,
                "round history scheduled for save"
            );
        }
        (changed || !history_path.exists()).then(|| history.clone())
    };

    let Some(history_to_save) = history_to_save else {
        return;
    };

    let history_base_path = state.round_history_path.clone();
    let log_history_path = history_path.clone();
    match tokio::task::spawn_blocking(move || {
        save_round_history_merged(&history_base_path, &chain_id, &history_to_save, &retention)
    })
    .await
    {
        Ok(Ok(saved_history)) => {
            state.round_history.write().await.merge_from(saved_history);
        }
        Ok(Err(error)) => {
            warn!(
                path = %log_history_path.display(),
                error = ?error,
                "failed to save round history"
            );
        }
        Err(error) => {
            warn!(
                path = %log_history_path.display(),
                error = ?error,
                "round history save task failed"
            );
        }
    }
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
