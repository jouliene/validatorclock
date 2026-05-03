use crate::config::{AppConfig, ChainConfig};
use crate::fsutil::write_file_atomic;
use crate::history::{
    RecentAbsentValidatorDto, ValidatorParticipationDto, load_round_history,
    save_round_history_merged,
};
use crate::state::AppState;
use anyhow::{Context, Result, anyhow, bail};
use minik2::{
    Config, CurrentElectionData, Elector, FpTokens, HashBytes, JrpcTransport, Ref, Transport,
    ValidatorSet, apply_price_factor,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::env;
use std::fs;
use std::io::ErrorKind;
use std::path::Path;
use std::sync::{Arc, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use tokio::time::{Duration, MissedTickBehavior, interval, timeout};
use tracing::{debug, info, warn};
use tycho_types::abi::{AbiType, AbiValue, AbiVersion, FromAbi, Function, WithAbiType};
use tycho_types::boc::BocRepr;
use tycho_types::models::account::AccountState;
use tycho_types::models::{IntAddr, MsgInfo, Transaction};
use tycho_types::num::Tokens;

const ELECTOR_TX_SCAN_LIMIT: u8 = 100;
const ONE_TOKEN: u128 = 1_000_000_000;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ChainsResponse {
    refresh_seconds: u64,
    chains: Vec<ChainMeta>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct RuntimeStatusResponse {
    status: &'static str,
    version: &'static str,
    started_at: Option<u64>,
    uptime_seconds: u64,
    refresh_seconds: u64,
    refresh_timeout_seconds: u64,
    chains: Vec<ChainRuntimeStatusDto>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct HistoryBackfillReport {
    chain_id: String,
    history_path: String,
    rounds_requested: usize,
    max_pages_per_round: usize,
    total_pages_scanned: usize,
    all_windows_reached_stop: bool,
    rounds_scanned: usize,
    rounds_recorded: usize,
    scanned_rounds: Vec<HistoryBackfillRoundReport>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct HistoryBackfillRoundReport {
    stake_at: u32,
    round_id: u32,
    round_color: RoundColor,
    pages_scanned: usize,
    reached_stop: bool,
    validators_found: usize,
    recorded: bool,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ChainMeta {
    id: String,
    name: String,
    color: String,
    token_symbol: String,
    rpc_label: String,
}

#[derive(Debug, Clone, Serialize)]
struct ChainRuntimeStatusDto {
    id: String,
    name: String,
    cached: bool,
    fetched_at: Option<u64>,
    age_seconds: Option<u64>,
    stale: bool,
    last_attempt_at: Option<u64>,
    last_success_at: Option<u64>,
    last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ClockSnapshot {
    pub(crate) chain: ChainMeta,
    pub(crate) fetched_at: u64,
    pub(crate) global_id: i32,
    pub(crate) seqno: u32,
    pub(crate) params15: ElectionTimingsDto,
    pub(crate) current_set: ValidatorSetDto,
    pub(crate) previous_set: Option<ValidatorSetDto>,
    pub(crate) next_set: Option<ValidatorSetDto>,
    pub(crate) election: ElectionDto,
    pub(crate) warning: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ElectionTimingsDto {
    validators_elected_for: u32,
    elections_start_before: u32,
    elections_end_before: u32,
    stake_held_for: u32,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ValidatorSetDto {
    pub(crate) utime_since: u32,
    pub(crate) utime_until: u32,
    pub(crate) round_id: u32,
    pub(crate) round_color: RoundColor,
    pub(crate) total: usize,
    pub(crate) main: u16,
    pub(crate) total_weight: String,
    pub(crate) total_stake: Option<String>,
    pub(crate) total_reward: Option<String>,
    pub(crate) validators: Vec<ValidatorDto>,
    pub(crate) recent_absent_validators: Vec<RecentAbsentValidatorDto>,
}

#[derive(Debug, Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum RoundColor {
    Blue,
    Green,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ValidatorDto {
    pub(crate) public_key: String,
    pub(crate) adnl_addr: Option<String>,
    pub(crate) wallet: Option<String>,
    pub(crate) stake: Option<String>,
    pub(crate) reward: Option<String>,
    pub(crate) weight: String,
    pub(crate) weight_percent: f64,
    pub(crate) history: Vec<ValidatorParticipationDto>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub(crate) struct ElectionDto {
    pub(crate) candidates: Vec<ElectionCandidateDto>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ElectionCandidateDto {
    pub(crate) public_key: String,
    stake: String,
    stake_raw: String,
    created_at: u32,
    stake_factor: u32,
    pub(crate) wallet: String,
    adnl_addr: String,
    pub(crate) history: Vec<ValidatorParticipationDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ValidatorElectionHistory {
    wallet: String,
    stake: String,
    #[serde(default)]
    reward: Option<String>,
    #[serde(default)]
    weight: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct ValidatorRoundData {
    #[serde(default)]
    validators: ValidatorHistory,
    #[serde(default)]
    total_stake: Option<String>,
    #[serde(default)]
    total_stake_raw: Option<String>,
    #[serde(default)]
    total_reward: Option<String>,
    #[serde(default)]
    total_reward_raw: Option<String>,
    #[serde(default)]
    total_weight_raw: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ValidatorRoundDiskCache {
    version: u32,
    rounds: HashMap<String, ValidatorRoundData>,
}

#[derive(Debug, Clone, FromAbi, WithAbiType)]
#[allow(dead_code)]
struct ParticipateInElectionsInput {
    query_id: u64,
    validator_key: minik2::HashBytes,
    stake_at: u32,
    stake_factor: u32,
    adnl_addr: minik2::HashBytes,
    signature: Vec<u8>,
}

#[derive(Debug, Clone, FromAbi, WithAbiType)]
#[allow(dead_code)]
struct FullElectorData {
    current_election: Option<Ref<CurrentElectionData>>,
    credits: BTreeMap<HashBytes, FpTokens>,
    past_elections: BTreeMap<u32, FullPastElectionData>,
    grams: Tokens,
    active_id: u32,
    active_hash: HashBytes,
}

#[derive(Debug, Clone, FromAbi, WithAbiType)]
#[allow(dead_code)]
struct FullPastElectionData {
    unfreeze_at: u32,
    stake_held: u32,
    vset_hash: HashBytes,
    frozen_dict: BTreeMap<HashBytes, FrozenValidator>,
    total_stake: FpTokens,
    bonuses: FpTokens,
}

#[derive(Debug, Clone, FromAbi, WithAbiType)]
struct FrozenValidator {
    addr: HashBytes,
    weight: u64,
    stake: FpTokens,
}

#[derive(Debug, Clone)]
pub(crate) struct CacheEntry {
    fetched_at: u64,
    snapshot: ClockSnapshot,
}

type ValidatorHistory = HashMap<String, ValidatorElectionHistory>;
pub(crate) type ValidatorRoundCache = RwLock<HashMap<String, ValidatorRoundData>>;

struct ValidatorRoundHistoryScan<'a> {
    stake_at: u32,
    target_keys: Option<&'a HashSet<String>>,
    elections_start_before: u32,
    elections_end_before: u32,
    max_pages: usize,
    election_fee: u128,
    debug_history: bool,
    progress: bool,
    align_to_window: bool,
}

#[derive(Debug)]
struct ValidatorRoundHistoryScanResult {
    round_data: ValidatorRoundData,
    pages_scanned: usize,
    reached_stop: bool,
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
        match get_chain_snapshot(state, &chain_id, true).await {
            Ok(snapshot) if snapshot.warning.is_some() => {
                debug!(chain_id, warning = ?snapshot.warning, "background refresh used cached data");
            }
            Ok(snapshot) => {
                debug!(
                    chain_id,
                    fetched_at = snapshot.fetched_at,
                    "background refresh completed"
                );
            }
            Err(error) => {
                warn!(chain_id, error = ?error, "background refresh failed");
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
        fetch_chain_snapshot_cached(
            chain,
            &state.validator_round_cache,
            &state.validator_round_cache_path,
        ),
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
    let mut history = state.round_history.write().await;
    history.record_snapshot(&chain_id, snapshot, observed_at);
    history.annotate_snapshot(&chain_id, snapshot);
    if let Err(error) = save_round_history_merged(&state.round_history_path, &mut history) {
        warn!(
            path = %state.round_history_path.display(),
            error = ?error,
            "failed to save round history"
        );
    }
}

pub(crate) async fn fetch_chain_snapshot(chain: &ChainConfig) -> Result<ClockSnapshot> {
    fetch_chain_snapshot_inner(chain, None, None).await
}

pub(crate) async fn backfill_round_history(
    config: &AppConfig,
    chain_id: &str,
    rounds: usize,
    max_pages: usize,
) -> Result<HistoryBackfillReport> {
    if rounds == 0 {
        bail!("rounds must be greater than zero");
    }
    if max_pages == 0 {
        bail!("max_pages must be greater than zero");
    }

    let chain = config
        .chain(chain_id)
        .ok_or_else(|| anyhow!("unknown chain id `{chain_id}`"))?;
    let transport = Transport::jrpc(&chain.rpc)
        .with_context(|| format!("invalid RPC endpoint for `{}`", chain.id))?;
    let network_config = Config::fetch(&transport)
        .await
        .with_context(|| format!("failed to fetch config from `{}`", chain.id))?;
    let timings = network_config.election_timings()?;
    let current_set = network_config.current_validator_set()?;
    let capabilities = transport.get_capabilities().await.unwrap_or_default();
    if !capabilities
        .iter()
        .any(|capability| capability == "getTransactionsList")
    {
        bail!("RPC endpoint for `{chain_id}` does not support getTransactionsList");
    }

    let elector = Elector::from_config(&transport, &network_config)?;
    let rpc = JrpcTransport::new(&chain.rpc)?;
    let account = elector.address().to_string();
    let election_fee = apply_price_factor(ONE_TOKEN, network_config.compute_price_factor(true)?);
    let debug_history = env::var_os("VALIDATORS_CLOCK_DEBUG_HISTORY").is_some();
    let history_path = config.effective_history_path();
    let mut history = load_round_history(&history_path)?;
    let observed_at = now_sec()?;

    let mut targets = Vec::new();
    for offset in 1..=rounds {
        let Some(round_offset) = (offset as u32).checked_mul(timings.validators_elected_for) else {
            break;
        };
        let Some(stake_at) = current_set.utime_since.checked_sub(round_offset) else {
            break;
        };
        targets.push(stake_at);
    }

    let mut scanned_rounds = Vec::new();
    for stake_at in targets {
        let scan_result = scan_validator_election_round_history(
            &rpc,
            &account,
            ValidatorRoundHistoryScan {
                stake_at,
                target_keys: None,
                elections_start_before: timings.elections_start_before,
                elections_end_before: timings.elections_end_before,
                max_pages,
                election_fee,
                debug_history,
                progress: true,
                align_to_window: true,
            },
        )
        .await?;
        let round_data = scan_result.round_data;
        let round_id = stake_at / timings.validators_elected_for.max(1);
        let validators_found = round_data.validators.len();
        let set =
            ValidatorSetDto::from_round_data(stake_at, timings.validators_elected_for, &round_data);
        let recorded = if scan_result.reached_stop
            && let Some(set) = set
        {
            history.record_validator_set(chain_id, &set, observed_at, false);
            save_round_history_merged(&history_path, &mut history)?;
            true
        } else {
            false
        };

        scanned_rounds.push(HistoryBackfillRoundReport {
            stake_at,
            round_id,
            round_color: round_color(round_id),
            pages_scanned: scan_result.pages_scanned,
            reached_stop: scan_result.reached_stop,
            validators_found,
            recorded,
        });
    }

    let rounds_recorded = scanned_rounds.iter().filter(|round| round.recorded).count();
    save_round_history_merged(&history_path, &mut history)?;

    Ok(HistoryBackfillReport {
        chain_id: chain_id.to_owned(),
        history_path: history_path.display().to_string(),
        rounds_requested: rounds,
        max_pages_per_round: max_pages,
        total_pages_scanned: scanned_rounds.iter().map(|round| round.pages_scanned).sum(),
        all_windows_reached_stop: scanned_rounds.iter().all(|round| round.reached_stop),
        rounds_scanned: scanned_rounds.len(),
        rounds_recorded,
        scanned_rounds,
    })
}

async fn fetch_chain_snapshot_cached(
    chain: &ChainConfig,
    validator_round_cache: &ValidatorRoundCache,
    validator_round_cache_path: &Path,
) -> Result<ClockSnapshot> {
    fetch_chain_snapshot_inner(
        chain,
        Some(validator_round_cache),
        Some(validator_round_cache_path),
    )
    .await
}

async fn fetch_chain_snapshot_inner(
    chain: &ChainConfig,
    validator_round_cache: Option<&ValidatorRoundCache>,
    validator_round_cache_path: Option<&Path>,
) -> Result<ClockSnapshot> {
    let transport = Transport::jrpc(&chain.rpc)
        .with_context(|| format!("invalid RPC endpoint for `{}`", chain.id))?;
    let config = Config::fetch(&transport)
        .await
        .with_context(|| format!("failed to fetch config from `{}`", chain.id))?;
    let timings = config.election_timings()?;
    let current_set = config.current_validator_set()?;
    let next_set = config.next_validator_set()?;
    let election = fetch_election(&transport, &config)
        .await
        .unwrap_or_default();
    let validator_round_data_result = fetch_validator_round_data(
        chain,
        &transport,
        &config,
        &current_set,
        next_set.as_ref(),
        validator_round_cache,
        validator_round_cache_path,
    )
    .await;
    let validator_round_data = match validator_round_data_result {
        Ok(round_data) => round_data,
        Err(error) => {
            if env::var_os("VALIDATORS_CLOCK_DEBUG_HISTORY").is_some() {
                debug!(error = ?error, "validator round data failed");
            }
            HashMap::new()
        }
    };

    Ok(ClockSnapshot {
        chain: ChainMeta::from(chain),
        fetched_at: now_sec()?,
        global_id: config.global_id(),
        seqno: config.seqno(),
        params15: ElectionTimingsDto {
            validators_elected_for: timings.validators_elected_for,
            elections_start_before: timings.elections_start_before,
            elections_end_before: timings.elections_end_before,
            stake_held_for: timings.stake_held_for,
        },
        current_set: ValidatorSetDto::from_set(
            &current_set,
            timings.validators_elected_for,
            validator_round_data.get(&current_set.utime_since),
        ),
        previous_set: previous_validator_set(
            &current_set,
            timings.validators_elected_for,
            &validator_round_data,
        ),
        next_set: next_set.as_ref().map(|set| {
            ValidatorSetDto::from_set(
                set,
                timings.validators_elected_for,
                validator_round_data.get(&set.utime_since),
            )
        }),
        election,
        warning: None,
    })
}

impl From<&ChainConfig> for ChainMeta {
    fn from(chain: &ChainConfig) -> Self {
        Self {
            id: chain.id.clone(),
            name: chain.name.clone(),
            color: chain.color.clone(),
            token_symbol: chain.token_symbol.clone(),
            rpc_label: chain
                .rpc_label
                .clone()
                .unwrap_or_else(|| endpoint_label(&chain.rpc)),
        }
    }
}

impl ValidatorSetDto {
    fn from_set(
        set: &ValidatorSet,
        validators_elected_for: u32,
        round_data: Option<&ValidatorRoundData>,
    ) -> Self {
        let round_id = set.utime_since / validators_elected_for.max(1);
        let total_weight = set.total_weight.max(1);
        let total_weight_raw = total_weight as u128;
        let total_reward_raw = round_data
            .and_then(|data| data.total_reward_raw.as_deref())
            .and_then(|value| value.parse::<u128>().ok());
        let validator_history = round_data.map(|data| &data.validators);
        Self {
            utime_since: set.utime_since,
            utime_until: set.utime_until,
            round_id,
            round_color: round_color(round_id),
            total: set.list.len(),
            main: set.main.get(),
            total_weight: set.total_weight.to_string(),
            total_stake: round_data.and_then(|data| data.total_stake.clone()),
            total_reward: round_data.and_then(|data| data.total_reward.clone()),
            validators: set
                .list
                .iter()
                .map(|validator| {
                    let public_key = hex_lower(&validator.public_key.0);
                    let history = validator_history.and_then(|history| history.get(&public_key));
                    ValidatorDto {
                        public_key,
                        adnl_addr: validator.adnl_addr.as_ref().map(|adnl| hex_lower(&adnl.0)),
                        wallet: history.map(|history| history.wallet.clone()),
                        stake: history.map(|history| history.stake.clone()),
                        reward: total_reward_raw
                            .map(|reward| {
                                FpTokens(
                                    reward.saturating_mul(validator.weight as u128)
                                        / total_weight_raw,
                                )
                                .to_string()
                            })
                            .or_else(|| history.and_then(|history| history.reward.clone())),
                        weight: validator.weight.to_string(),
                        weight_percent: validator.weight as f64 * 100.0 / total_weight as f64,
                        history: Vec::new(),
                    }
                })
                .collect(),
            recent_absent_validators: Vec::new(),
        }
    }

    fn from_round_data(
        stake_at: u32,
        validators_elected_for: u32,
        round_data: &ValidatorRoundData,
    ) -> Option<Self> {
        if round_data.validators.is_empty() {
            return None;
        }

        let total_weight_raw = round_data
            .total_weight_raw
            .as_deref()
            .and_then(|value| value.parse::<u128>().ok())
            .unwrap_or_else(|| {
                round_data
                    .validators
                    .values()
                    .filter_map(|validator| validator.weight.as_deref())
                    .filter_map(|weight| weight.parse::<u128>().ok())
                    .sum()
            });
        let total_weight = total_weight_raw.max(1);
        let mut validators: Vec<_> = round_data
            .validators
            .iter()
            .map(|(public_key, history)| {
                let weight = history.weight.clone().unwrap_or_else(|| "0".to_owned());
                let weight_raw = weight.parse::<u128>().unwrap_or(0);
                ValidatorDto {
                    public_key: public_key.clone(),
                    adnl_addr: None,
                    wallet: Some(history.wallet.clone()),
                    stake: Some(history.stake.clone()),
                    reward: history.reward.clone(),
                    weight,
                    weight_percent: weight_raw as f64 * 100.0 / total_weight as f64,
                    history: Vec::new(),
                }
            })
            .collect();
        validators.sort_by(|a, b| {
            b.weight
                .parse::<u128>()
                .unwrap_or(0)
                .cmp(&a.weight.parse::<u128>().unwrap_or(0))
                .then_with(|| a.public_key.cmp(&b.public_key))
        });

        let total = validators.len();
        let round_id = stake_at / validators_elected_for.max(1);
        Some(Self {
            utime_since: stake_at,
            utime_until: stake_at.saturating_add(validators_elected_for),
            round_id,
            round_color: round_color(round_id),
            total,
            main: total.min(u16::MAX as usize) as u16,
            total_weight: total_weight_raw.to_string(),
            total_stake: round_data.total_stake.clone(),
            total_reward: round_data.total_reward.clone(),
            validators,
            recent_absent_validators: Vec::new(),
        })
    }
}

fn previous_validator_set(
    current_set: &ValidatorSet,
    validators_elected_for: u32,
    validator_round_data: &HashMap<u32, ValidatorRoundData>,
) -> Option<ValidatorSetDto> {
    let previous_stake_at = current_set
        .utime_since
        .checked_sub(validators_elected_for)?;
    let round_data = validator_round_data.get(&previous_stake_at)?;
    ValidatorSetDto::from_round_data(previous_stake_at, validators_elected_for, round_data)
}

async fn fetch_election(transport: &Transport, config: &Config) -> Result<ElectionDto> {
    let elector = Elector::from_config(transport, config)?;
    let data = elector.get_data().await?;
    let Some(current) = data.current_election() else {
        return Ok(ElectionDto::default());
    };

    Ok(ElectionDto {
        candidates: current
            .members
            .iter()
            .map(|(public_key, member)| ElectionCandidateDto {
                public_key: hex_lower(&public_key.0),
                stake: member.msg_value.to_string(),
                stake_raw: member.msg_value.0.to_string(),
                created_at: member.created_at,
                stake_factor: member.stake_factor,
                wallet: masterchain_hash_address(&member.src_addr.0),
                adnl_addr: hex_lower(&member.adnl_addr.0),
                history: Vec::new(),
            })
            .collect(),
    })
}

async fn fetch_frozen_validator_round_data(
    transport: &Transport,
    config: &Config,
) -> Result<HashMap<u32, ValidatorRoundData>> {
    let data = fetch_full_elector_data(transport, config).await?;
    Ok(data
        .past_elections
        .iter()
        .map(|(stake_at, election)| (*stake_at, validator_round_data_from_frozen(election)))
        .collect())
}

async fn fetch_full_elector_data(
    transport: &Transport,
    config: &Config,
) -> Result<FullElectorData> {
    let elector = Elector::from_config(transport, config)?;
    let state = transport
        .get_account_state(elector.address().to_string())
        .await?;
    let account = state.account().context("elector account not found")?;

    let AccountState::Active(state_init) = &account.state else {
        bail!("elector account is not active");
    };
    let data = state_init.data.as_ref().context("elector data is empty")?;

    AbiValue::load_partial(
        full_elector_data_abi(),
        AbiVersion::V2_1,
        &mut data.as_slice()?,
    )
    .and_then(FullElectorData::from_abi)
    .context("failed to parse full elector data")
}

fn validator_round_data_from_frozen(election: &FullPastElectionData) -> ValidatorRoundData {
    let total_weight = election
        .frozen_dict
        .values()
        .fold(0_u128, |sum, validator| {
            sum.saturating_add(validator.weight as u128)
        })
        .max(1);
    let validators = election
        .frozen_dict
        .iter()
        .map(|(public_key, validator)| {
            let reward = election.bonuses.0.saturating_mul(validator.weight as u128) / total_weight;
            (
                hex_lower(&public_key.0),
                ValidatorElectionHistory {
                    wallet: masterchain_hash_address(&validator.addr.0),
                    stake: validator.stake.to_string(),
                    reward: Some(FpTokens(reward).to_string()),
                    weight: Some(validator.weight.to_string()),
                },
            )
        })
        .collect();

    ValidatorRoundData {
        validators,
        total_stake: Some(election.total_stake.to_string()),
        total_stake_raw: Some(election.total_stake.0.to_string()),
        total_reward: Some(election.bonuses.to_string()),
        total_reward_raw: Some(election.bonuses.0.to_string()),
        total_weight_raw: Some(total_weight.to_string()),
    }
}

async fn fetch_validator_round_data(
    chain: &ChainConfig,
    transport: &Transport,
    config: &Config,
    current_set: &ValidatorSet,
    next_set: Option<&ValidatorSet>,
    validator_round_cache: Option<&ValidatorRoundCache>,
    validator_round_cache_path: Option<&Path>,
) -> Result<HashMap<u32, ValidatorRoundData>> {
    let mut rounds = fetch_frozen_validator_round_data(transport, config)
        .await
        .unwrap_or_else(|error| {
            if env::var_os("VALIDATORS_CLOCK_DEBUG_HISTORY").is_some() {
                debug!(error = ?error, "frozen validator round data unavailable");
            }
            HashMap::new()
        });
    let missing_targets: Vec<_> = validator_round_targets(current_set, next_set)
        .into_iter()
        .filter(|(stake_at, target_keys)| {
            !target_keys.is_empty()
                && rounds
                    .get(stake_at)
                    .is_none_or(|round| round.validators.len() < target_keys.len())
        })
        .collect();
    if missing_targets.is_empty() {
        return Ok(rounds);
    }

    let capabilities = transport.get_capabilities().await.unwrap_or_default();
    if !capabilities
        .iter()
        .any(|capability| capability == "getTransactionsList")
    {
        return Ok(rounds);
    }

    let elector = Elector::from_config(transport, config)?;
    let rpc = JrpcTransport::new(&chain.rpc)?;
    let account = elector.address().to_string();
    let timings = config.election_timings()?;
    let election_fee = apply_price_factor(ONE_TOKEN, config.compute_price_factor(true)?);
    let debug_history = env::var_os("VALIDATORS_CLOCK_DEBUG_HISTORY").is_some();

    for (stake_at, target_keys) in missing_targets {
        let cache_key = validator_history_cache_key(chain, stake_at);
        if let Some(cache) = validator_round_cache
            && let Some(cached) = cache.read().await.get(&cache_key)
        {
            merge_validator_round_data(rounds.entry(stake_at).or_default(), cached.clone());
            continue;
        }

        let round_data = scan_validator_election_round_history(
            &rpc,
            &account,
            ValidatorRoundHistoryScan {
                stake_at,
                target_keys: Some(&target_keys),
                elections_start_before: timings.elections_start_before,
                elections_end_before: timings.elections_end_before,
                max_pages: 700,
                election_fee,
                debug_history,
                progress: false,
                align_to_window: false,
            },
        )
        .await?;
        let round_data = round_data.round_data;

        if let Some(cache) = validator_round_cache {
            let snapshot = {
                let mut cache = cache.write().await;
                cache.insert(cache_key, round_data.clone());
                cache.clone()
            };
            if let Some(path) = validator_round_cache_path
                && let Err(error) = save_validator_round_disk_cache(path, &snapshot)
            {
                warn!(path = %path.display(), error = ?error, "failed to save validator round cache");
            }
        }
        merge_validator_round_data(rounds.entry(stake_at).or_default(), round_data);
    }

    Ok(rounds)
}

fn merge_validator_round_data(target: &mut ValidatorRoundData, source: ValidatorRoundData) {
    target.validators.extend(source.validators);
    if target.total_stake.is_none() {
        target.total_stake = source.total_stake;
    }
    if target.total_stake_raw.is_none() {
        target.total_stake_raw = source.total_stake_raw;
    }
    if target.total_reward.is_none() {
        target.total_reward = source.total_reward;
    }
    if target.total_reward_raw.is_none() {
        target.total_reward_raw = source.total_reward_raw;
    }
    if target.total_weight_raw.is_none() {
        target.total_weight_raw = source.total_weight_raw;
    }
}

async fn scan_validator_election_round_history(
    rpc: &JrpcTransport,
    account: &str,
    scan: ValidatorRoundHistoryScan<'_>,
) -> Result<ValidatorRoundHistoryScanResult> {
    let election_end_at = scan.stake_at.saturating_sub(scan.elections_end_before);
    let election_start_at = scan.stake_at.saturating_sub(scan.elections_start_before);
    let mut continuation = if scan.align_to_window {
        estimated_transaction_lt_in_window(rpc, account, election_end_at, election_start_at).await?
    } else {
        estimated_transaction_lt_at(rpc, account, election_end_at).await?
    };
    let mut history = HashMap::new();
    let mut pages_scanned = 0_usize;
    let mut reached_stop = false;

    for page in 0..scan.max_pages {
        let mut params = serde_json::json!({
            "account": account,
            "limit": ELECTOR_TX_SCAN_LIMIT,
        });
        if let Some(lt) = &continuation {
            params["lastTransactionLt"] = serde_json::json!(lt);
        }

        let tx_bocs: Vec<String> = rpc.call("getTransactionsList", params).await?;
        if tx_bocs.is_empty() {
            break;
        }
        pages_scanned += 1;

        let mut next_continuation = None;
        let mut newest_now = None::<u32>;
        let mut oldest_now = None::<u32>;
        for tx_boc in tx_bocs {
            let transaction: Transaction = BocRepr::decode_base64(tx_boc)?;
            newest_now = Some(newest_now.map_or(transaction.now, |now| now.max(transaction.now)));
            oldest_now = Some(oldest_now.map_or(transaction.now, |now| now.min(transaction.now)));
            next_continuation = Some(transaction.prev_trans_lt.to_string());
            if transaction.now < election_start_at {
                reached_stop = true;
                continue;
            }
            if !transaction_is_inside_election_window(
                transaction.now,
                election_start_at,
                election_end_at,
            ) {
                continue;
            }

            let Some(submission) = parse_election_submission(&transaction, scan.election_fee)?
            else {
                continue;
            };
            if scan.debug_history {
                debug!(
                    round = scan.stake_at,
                    page,
                    now = transaction.now,
                    stake_at = submission.stake_at,
                    public_key = %submission.public_key,
                    wallet = %submission.wallet,
                    stake = %submission.stake,
                    "election transaction"
                );
            }
            if submission.stake_at != scan.stake_at {
                continue;
            }
            if let Some(target_keys) = scan.target_keys
                && !target_keys.contains(&submission.public_key)
            {
                continue;
            }

            history
                .entry(submission.public_key)
                .or_insert(ValidatorElectionHistory {
                    wallet: submission.wallet,
                    stake: submission.stake,
                    reward: None,
                    weight: None,
                });
        }

        if scan.progress && (pages_scanned == 1 || pages_scanned.is_multiple_of(10)) {
            eprintln!(
                "round {} scan progress: pages={} validators_found={} reached_stop={}",
                scan.stake_at,
                pages_scanned,
                history.len(),
                reached_stop
            );
            if let (Some(newest_now), Some(oldest_now)) = (newest_now, oldest_now) {
                eprintln!(
                    "round {} page time range: newest_now={} oldest_now={} election_end={} election_start={}",
                    scan.stake_at, newest_now, oldest_now, election_end_at, election_start_at
                );
            }
        }
        if scan.debug_history {
            debug!(
                round = scan.stake_at,
                page,
                mapped = history.len(),
                targets = scan.target_keys.map(HashSet::len),
                continuation = ?next_continuation,
                "history scan progress"
            );
        }
        if scan
            .target_keys
            .is_some_and(|target_keys| history.len() >= target_keys.len())
            || reached_stop
        {
            break;
        }
        if next_continuation.as_deref() == Some("0") || next_continuation == continuation {
            break;
        }
        continuation = next_continuation;
    }

    Ok(ValidatorRoundHistoryScanResult {
        round_data: ValidatorRoundData {
            validators: history,
            total_stake: None,
            total_stake_raw: None,
            total_reward: None,
            total_reward_raw: None,
            total_weight_raw: None,
        },
        pages_scanned,
        reached_stop,
    })
}

fn validator_round_targets(
    current_set: &ValidatorSet,
    next_set: Option<&ValidatorSet>,
) -> Vec<(u32, HashSet<String>)> {
    let mut targets = vec![(current_set.utime_since, validator_public_keys(current_set))];
    if let Some(next_set) = next_set {
        targets.push((next_set.utime_since, validator_public_keys(next_set)));
    }
    targets
}

fn validator_public_keys(set: &ValidatorSet) -> HashSet<String> {
    set.list
        .iter()
        .map(|validator| hex_lower(&validator.public_key.0))
        .collect()
}

fn validator_history_cache_key(chain: &ChainConfig, stake_at: u32) -> String {
    format!("{}:{}:{}", chain.id, chain.rpc, stake_at)
}

async fn estimated_transaction_lt_at(
    rpc: &JrpcTransport,
    account: &str,
    unix_time: u32,
) -> Result<Option<String>> {
    let tx_bocs: Vec<String> = rpc
        .call(
            "getTransactionsList",
            serde_json::json!({
                "account": account,
                "limit": 1,
            }),
        )
        .await?;
    let Some(tx_boc) = tx_bocs.first() else {
        return Ok(None);
    };
    let transaction: Transaction = BocRepr::decode_base64(tx_boc)?;
    let seconds_back = transaction.now.saturating_sub(unix_time) as u64;
    let estimated_lt = transaction
        .lt
        .saturating_sub(seconds_back.saturating_mul(2_000_000));
    Ok(Some(estimated_lt.to_string()))
}

async fn estimated_transaction_lt_in_window(
    rpc: &JrpcTransport,
    account: &str,
    election_end_at: u32,
    election_start_at: u32,
) -> Result<Option<String>> {
    let Some(latest) = fetch_transaction_before_lt(rpc, account, None).await? else {
        return Ok(None);
    };
    let seconds_back = latest.now.saturating_sub(election_end_at) as u64;
    if seconds_back == 0 {
        return Ok(Some(latest.lt.saturating_add(1).to_string()));
    }

    let mut low = 0_u64;
    let mut high = 4_000_000_u64;
    let mut best = None::<(u32, u64)>;

    for _ in 0..32 {
        if low > high {
            break;
        }
        let factor = low.saturating_add((high - low) / 2);
        let lt = latest
            .lt
            .saturating_sub(seconds_back.saturating_mul(factor));
        let Some(tx) = fetch_transaction_before_lt(rpc, account, Some(lt)).await? else {
            high = factor.saturating_sub(1);
            continue;
        };

        best = Some(match best {
            Some((best_distance, best_lt)) => {
                let distance = window_time_distance(tx.now, election_start_at, election_end_at);
                if distance < best_distance {
                    (distance, lt)
                } else {
                    (best_distance, best_lt)
                }
            }
            None => {
                let distance = window_time_distance(tx.now, election_start_at, election_end_at);
                (distance, lt)
            }
        });

        if tx.now > election_end_at {
            low = factor.saturating_add(1);
        } else if tx.now < election_start_at {
            high = factor.saturating_sub(1);
        } else {
            return Ok(Some(lt.to_string()));
        }
    }

    Ok(best.map(|(_, lt)| lt.to_string()))
}

fn transaction_is_inside_election_window(
    now: u32,
    election_start_at: u32,
    election_end_at: u32,
) -> bool {
    now >= election_start_at && now <= election_end_at
}

fn window_time_distance(now: u32, election_start_at: u32, election_end_at: u32) -> u32 {
    if now > election_end_at {
        now - election_end_at
    } else {
        election_start_at.saturating_sub(now)
    }
}

async fn fetch_transaction_before_lt(
    rpc: &JrpcTransport,
    account: &str,
    last_transaction_lt: Option<u64>,
) -> Result<Option<Transaction>> {
    let mut params = serde_json::json!({
        "account": account,
        "limit": 1,
    });
    if let Some(lt) = last_transaction_lt {
        params["lastTransactionLt"] = serde_json::json!(lt.to_string());
    }

    let tx_bocs: Vec<String> = rpc.call("getTransactionsList", params).await?;
    tx_bocs
        .first()
        .map(|tx_boc| BocRepr::decode_base64(tx_boc).map_err(Into::into))
        .transpose()
}

#[derive(Debug)]
struct ElectionSubmission {
    public_key: String,
    wallet: String,
    stake: String,
    stake_at: u32,
}

fn parse_election_submission(
    transaction: &Transaction,
    election_fee: u128,
) -> Result<Option<ElectionSubmission>> {
    let Some(message) = transaction.load_in_msg()? else {
        return Ok(None);
    };

    let MsgInfo::Int(info) = message.info else {
        return Ok(None);
    };
    let Some(wallet) = std_addr_string(&info.src) else {
        return Ok(None);
    };

    let values = match participate_in_elections_fn().decode_internal_input(message.body) {
        Ok(values) => values,
        Err(_) => return Ok(None),
    };
    let Some(value) = values.into_iter().next() else {
        return Ok(None);
    };
    let input = ParticipateInElectionsInput::from_abi(value.value)?;
    let stake_raw = info.value.tokens.into_inner().saturating_sub(election_fee);

    Ok(Some(ElectionSubmission {
        public_key: hex_lower(&input.validator_key.0),
        wallet,
        stake: minik2::FpTokens(stake_raw).to_string(),
        stake_at: input.stake_at,
    }))
}

fn std_addr_string(address: &IntAddr) -> Option<String> {
    address.as_std().map(ToString::to_string)
}

fn full_elector_data_abi() -> &'static AbiType {
    static ABI: OnceLock<AbiType> = OnceLock::new();
    ABI.get_or_init(FullElectorData::abi_type)
}

fn participate_in_elections_fn() -> &'static Function {
    static FUNCTION: OnceLock<Function> = OnceLock::new();
    FUNCTION.get_or_init(|| {
        Function::builder(AbiVersion::V2_0, "participate_in_elections")
            .with_id(0x4e73744b)
            .with_inputs([ParticipateInElectionsInput::abi_type().named("input")])
            .build()
    })
}

fn round_color(round_id: u32) -> RoundColor {
    if round_id.is_multiple_of(2) {
        RoundColor::Blue
    } else {
        RoundColor::Green
    }
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

fn masterchain_hash_address(bytes: &[u8]) -> String {
    format!("-1:{}", hex_lower(bytes))
}

pub(crate) fn load_validator_round_disk_cache(
    path: &Path,
) -> Result<HashMap<String, ValidatorRoundData>> {
    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(HashMap::new()),
        Err(error) => {
            return Err(error).with_context(|| format!("failed to read {}", path.display()));
        }
    };
    let cache: ValidatorRoundDiskCache = serde_json::from_str(&content)
        .with_context(|| format!("failed to parse {}", path.display()))?;
    Ok(cache.rounds)
}

fn save_validator_round_disk_cache(
    path: &Path,
    rounds: &HashMap<String, ValidatorRoundData>,
) -> Result<()> {
    let cache = ValidatorRoundDiskCache {
        version: 1,
        rounds: rounds.clone(),
    };
    let content = serde_json::to_string_pretty(&cache)?;
    write_file_atomic(path, content.as_bytes(), 0o644)
}

fn endpoint_label(endpoint: &str) -> String {
    endpoint
        .trim()
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_end_matches('/')
        .to_owned()
}

fn now_sec() -> Result<u64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system time is before UNIX epoch")?
        .as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn validator_round_disk_cache_loads_missing_file_as_empty() {
        let dir = test_dir("missing");
        let path = dir.join("nested").join("rounds.json");

        let loaded = load_validator_round_disk_cache(&path).unwrap();

        assert!(loaded.is_empty());
    }

    #[test]
    fn validator_round_disk_cache_writes_atomically_and_creates_parent_dir() {
        let dir = test_dir("write");
        let path = dir.join("nested").join("rounds.json");
        let mut rounds = HashMap::new();
        rounds.insert(
            "test-round".to_owned(),
            ValidatorRoundData {
                total_stake: Some("100".to_owned()),
                total_stake_raw: Some("100000000000".to_owned()),
                ..ValidatorRoundData::default()
            },
        );

        save_validator_round_disk_cache(&path, &rounds).unwrap();

        let loaded = load_validator_round_disk_cache(&path).unwrap();
        assert_eq!(loaded["test-round"].total_stake.as_deref(), Some("100"));
        assert!(!path.with_extension("tmp").exists());
    }

    #[test]
    fn frozen_round_data_builds_previous_validator_set() {
        let mut validators = HashMap::new();
        validators.insert(
            "aa".to_owned(),
            ValidatorElectionHistory {
                wallet: "-1:aa".to_owned(),
                stake: "10".to_owned(),
                reward: Some("1".to_owned()),
                weight: Some("100".to_owned()),
            },
        );
        validators.insert(
            "bb".to_owned(),
            ValidatorElectionHistory {
                wallet: "-1:bb".to_owned(),
                stake: "20".to_owned(),
                reward: Some("2".to_owned()),
                weight: Some("200".to_owned()),
            },
        );
        let round = ValidatorRoundData {
            validators,
            total_stake: Some("30".to_owned()),
            total_reward: Some("3".to_owned()),
            total_weight_raw: Some("300".to_owned()),
            ..ValidatorRoundData::default()
        };

        let set = ValidatorSetDto::from_round_data(200, 100, &round).unwrap();

        assert_eq!(set.round_id, 2);
        assert!(matches!(set.round_color, RoundColor::Blue));
        assert_eq!(set.utime_until, 300);
        assert_eq!(set.total, 2);
        assert_eq!(set.total_weight, "300");
        assert_eq!(set.validators[0].public_key, "bb");
        assert!((set.validators[0].weight_percent - 66.666_666).abs() < 0.001);
    }

    #[test]
    fn election_window_filter_is_inclusive() {
        assert!(!transaction_is_inside_election_window(99, 100, 200));
        assert!(transaction_is_inside_election_window(100, 100, 200));
        assert!(transaction_is_inside_election_window(150, 100, 200));
        assert!(transaction_is_inside_election_window(200, 100, 200));
        assert!(!transaction_is_inside_election_window(201, 100, 200));
    }

    fn test_dir(label: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = env::temp_dir().join(format!(
            "validators_clock_chain_test_{label}_{}_{}",
            std::process::id(),
            nanos
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }
}
