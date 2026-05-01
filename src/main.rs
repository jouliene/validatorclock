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
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::RwLock;
use tycho_types::abi::{AbiType, AbiValue, AbiVersion, FromAbi, Function, WithAbiType};
use tycho_types::boc::BocRepr;
use tycho_types::models::account::AccountState;
use tycho_types::models::{IntAddr, MsgInfo, Transaction};
use tycho_types::num::Tokens;

const INDEX_HTML: &str = include_str!("../public/index.html");
const STYLES_CSS: &str = include_str!("../public/styles.css");
const APP_JS: &str = include_str!("../public/app.js");
const DEFAULT_CONFIG: &str = include_str!("../validators_clock.json");
const ELECTOR_TX_SCAN_LIMIT: u8 = 100;
const ONE_TOKEN: u128 = 1_000_000_000;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse()?;
    let config = Arc::new(load_config(cli.config_path.as_ref())?);
    config.validate()?;

    if let Some(chain_id) = cli.once {
        let chain = config
            .chain(&chain_id)
            .ok_or_else(|| anyhow!("unknown chain id `{chain_id}`"))?;
        let snapshot = fetch_chain_snapshot(chain).await?;
        println!("{}", serde_json::to_string_pretty(&snapshot)?);
        return Ok(());
    }

    let state = Arc::new(AppState {
        config: Arc::clone(&config),
        cache: RwLock::new(HashMap::new()),
        validator_round_cache_path: config.cache_path.clone(),
        validator_round_cache: RwLock::new(
            load_validator_round_disk_cache(&config.cache_path).unwrap_or_else(|error| {
                eprintln!("failed to load validator round cache: {error:#}");
                HashMap::new()
            }),
        ),
    });

    let listener = TcpListener::bind(&config.listen)
        .await
        .with_context(|| format!("failed to bind {}", config.listen))?;
    println!("validators_clock listening on http://{}", config.listen);

    loop {
        let (stream, _) = listener.accept().await?;
        let state = Arc::clone(&state);
        tokio::spawn(async move {
            if let Err(error) = handle_connection(stream, state).await {
                eprintln!("request failed: {error:#}");
            }
        });
    }
}

#[derive(Debug)]
struct Cli {
    config_path: Option<PathBuf>,
    once: Option<String>,
}

impl Cli {
    fn parse() -> Result<Self> {
        let mut args = env::args().skip(1);
        let mut config_path = None;
        let mut once = None;

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--config" => {
                    let value = args
                        .next()
                        .ok_or_else(|| anyhow!("--config requires a path"))?;
                    config_path = Some(PathBuf::from(value));
                }
                "--once" => {
                    let value = args
                        .next()
                        .ok_or_else(|| anyhow!("--once requires a chain id"))?;
                    once = Some(value);
                }
                "--help" | "-h" => {
                    println!(
                        "Usage: validators_clock [--config validators_clock.json] [--once chain_id]"
                    );
                    std::process::exit(0);
                }
                value if !value.starts_with('-') && config_path.is_none() => {
                    config_path = Some(PathBuf::from(value));
                }
                other => bail!("unknown argument `{other}`"),
            }
        }

        Ok(Self { config_path, once })
    }
}

#[derive(Debug, Clone, Deserialize)]
struct AppConfig {
    #[serde(default = "default_listen")]
    listen: String,
    #[serde(default = "default_refresh_seconds")]
    refresh_seconds: u64,
    #[serde(default = "default_cache_path")]
    cache_path: PathBuf,
    chains: Vec<ChainConfig>,
}

impl AppConfig {
    fn validate(&self) -> Result<()> {
        if self.chains.is_empty() {
            bail!("config must contain at least one chain");
        }

        for chain in &self.chains {
            if chain.id.trim().is_empty() {
                bail!("chain id cannot be empty");
            }
            if chain.name.trim().is_empty() {
                bail!("chain `{}` has empty name", chain.id);
            }
            if chain.rpc.trim().is_empty() {
                bail!("chain `{}` has empty rpc", chain.id);
            }
        }

        Ok(())
    }

    fn chain(&self, id: &str) -> Option<&ChainConfig> {
        self.chains.iter().find(|chain| chain.id == id)
    }
}

#[derive(Debug, Clone, Deserialize)]
struct ChainConfig {
    id: String,
    name: String,
    rpc: String,
    #[serde(default = "default_chain_color")]
    color: String,
    #[serde(default = "default_token_symbol")]
    token_symbol: String,
    #[serde(default)]
    rpc_label: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct ChainsResponse {
    refresh_seconds: u64,
    chains: Vec<ChainMeta>,
}

#[derive(Debug, Clone, Serialize)]
struct ChainMeta {
    id: String,
    name: String,
    color: String,
    token_symbol: String,
    rpc_label: String,
}

#[derive(Debug, Clone, Serialize)]
struct ClockSnapshot {
    chain: ChainMeta,
    fetched_at: u64,
    global_id: i32,
    seqno: u32,
    params15: ElectionTimingsDto,
    current_set: ValidatorSetDto,
    next_set: Option<ValidatorSetDto>,
    election: ElectionDto,
    warning: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct ElectionTimingsDto {
    validators_elected_for: u32,
    elections_start_before: u32,
    elections_end_before: u32,
    stake_held_for: u32,
}

#[derive(Debug, Clone, Serialize)]
struct ValidatorSetDto {
    utime_since: u32,
    utime_until: u32,
    round_id: u32,
    round_color: RoundColor,
    total: usize,
    main: u16,
    total_weight: String,
    total_stake: Option<String>,
    total_reward: Option<String>,
    validators: Vec<ValidatorDto>,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
enum RoundColor {
    Blue,
    Green,
}

#[derive(Debug, Clone, Serialize)]
struct ValidatorDto {
    public_key: String,
    adnl_addr: Option<String>,
    wallet: Option<String>,
    stake: Option<String>,
    reward: Option<String>,
    weight: String,
    weight_percent: f64,
}

#[derive(Debug, Clone, Default, Serialize)]
struct ElectionDto {
    candidates: Vec<ElectionCandidateDto>,
}

#[derive(Debug, Clone, Serialize)]
struct ElectionCandidateDto {
    public_key: String,
    stake: String,
    stake_raw: String,
    created_at: u32,
    stake_factor: u32,
    wallet: String,
    adnl_addr: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ValidatorElectionHistory {
    wallet: String,
    stake: String,
    #[serde(default)]
    reward: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ValidatorRoundData {
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
struct CacheEntry {
    fetched_at: u64,
    snapshot: ClockSnapshot,
}

type ValidatorHistory = HashMap<String, ValidatorElectionHistory>;
type ValidatorRoundCache = RwLock<HashMap<String, ValidatorRoundData>>;

struct ValidatorRoundHistoryScan<'a> {
    stake_at: u32,
    target_keys: &'a HashSet<String>,
    elections_start_before: u32,
    elections_end_before: u32,
    election_fee: u128,
    debug_history: bool,
}

struct AppState {
    config: Arc<AppConfig>,
    cache: RwLock<HashMap<String, CacheEntry>>,
    validator_round_cache_path: PathBuf,
    validator_round_cache: ValidatorRoundCache,
}

async fn handle_connection(mut stream: TcpStream, state: Arc<AppState>) -> Result<()> {
    let request = read_request(&mut stream).await?;
    let response = route_request(&request, state).await;
    write_response(&mut stream, response).await?;
    Ok(())
}

async fn read_request(stream: &mut TcpStream) -> Result<HttpRequest> {
    let mut buffer = vec![0_u8; 8192];
    let mut read = 0;

    loop {
        let n = stream.read(&mut buffer[read..]).await?;
        if n == 0 {
            break;
        }
        read += n;

        if buffer[..read]
            .windows(4)
            .any(|window| window == b"\r\n\r\n")
            || read == buffer.len()
        {
            break;
        }
    }

    let text = std::str::from_utf8(&buffer[..read]).context("request is not valid UTF-8")?;
    let request_line = text
        .lines()
        .next()
        .ok_or_else(|| anyhow!("missing request line"))?;
    let mut parts = request_line.split_whitespace();
    let method = parts
        .next()
        .ok_or_else(|| anyhow!("missing request method"))?
        .to_owned();
    let target = parts
        .next()
        .ok_or_else(|| anyhow!("missing request target"))?
        .to_owned();

    Ok(HttpRequest { method, target })
}

#[derive(Debug)]
struct HttpRequest {
    method: String,
    target: String,
}

async fn route_request(request: &HttpRequest, state: Arc<AppState>) -> HttpResponse {
    if request.method == "OPTIONS" {
        return HttpResponse::empty(204);
    }

    if request.method != "GET" && request.method != "HEAD" {
        return json_error(405, "method not allowed");
    }

    let (path, query) = split_target(&request.target);
    let force_refresh = query.map(query_forces_refresh).unwrap_or(false);
    let response = match path {
        "/" | "/index.html" => HttpResponse::ok("text/html; charset=utf-8", INDEX_HTML.as_bytes()),
        "/styles.css" => HttpResponse::ok("text/css; charset=utf-8", STYLES_CSS.as_bytes()),
        "/app.js" => HttpResponse::ok("application/javascript; charset=utf-8", APP_JS.as_bytes()),
        "/api/chains" => json_response(&chains_response(&state.config)),
        "/api/health" => json_response(&serde_json::json!({ "status": "ok" })),
        _ => match chain_clock_id(path) {
            Some(chain_id) => match get_chain_snapshot(&state, chain_id, force_refresh).await {
                Ok(snapshot) => json_response(&snapshot),
                Err(error) => json_error(500, &error.to_string()),
            },
            None => json_error(404, "not found"),
        },
    };

    if request.method == "HEAD" {
        response.without_body()
    } else {
        response
    }
}

fn chain_clock_id(path: &str) -> Option<&str> {
    let prefix = "/api/chains/";
    let suffix = "/clock";
    path.strip_prefix(prefix)?.strip_suffix(suffix)
}

fn split_target(target: &str) -> (&str, Option<&str>) {
    target
        .split_once('?')
        .map_or((target, None), |(path, query)| (path, Some(query)))
}

fn query_forces_refresh(query: &str) -> bool {
    query.split('&').any(|part| {
        let (key, value) = part.split_once('=').unwrap_or((part, "1"));
        key == "refresh" && matches!(value, "1" | "true" | "yes")
    })
}

fn chains_response(config: &AppConfig) -> ChainsResponse {
    ChainsResponse {
        refresh_seconds: config.refresh_seconds,
        chains: config.chains.iter().map(ChainMeta::from).collect(),
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
        && let Some(entry) = state.cache.read().await.get(chain_id)
        && now.saturating_sub(entry.fetched_at) < refresh_seconds
    {
        return Ok(entry.snapshot.clone());
    }

    let chain = state
        .config
        .chain(chain_id)
        .ok_or_else(|| anyhow!("unknown chain id `{chain_id}`"))?;

    match fetch_chain_snapshot_cached(
        chain,
        &state.validator_round_cache,
        &state.validator_round_cache_path,
    )
    .await
    {
        Ok(snapshot) => {
            state.cache.write().await.insert(
                chain_id.to_owned(),
                CacheEntry {
                    fetched_at: now,
                    snapshot: snapshot.clone(),
                },
            );
            Ok(snapshot)
        }
        Err(error) => {
            if let Some(entry) = state.cache.read().await.get(chain_id) {
                let mut snapshot = entry.snapshot.clone();
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

async fn fetch_chain_snapshot(chain: &ChainConfig) -> Result<ClockSnapshot> {
    fetch_chain_snapshot_inner(chain, None, None).await
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
                eprintln!("validator round data failed: {error:#}");
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
                    }
                })
                .collect(),
        }
    }
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
                eprintln!("frozen validator round data unavailable: {error:#}");
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
                target_keys: &target_keys,
                elections_start_before: timings.elections_start_before,
                elections_end_before: timings.elections_end_before,
                election_fee,
                debug_history,
            },
        )
        .await?;

        if let Some(cache) = validator_round_cache {
            let snapshot = {
                let mut cache = cache.write().await;
                cache.insert(cache_key, round_data.clone());
                cache.clone()
            };
            if let Some(path) = validator_round_cache_path
                && let Err(error) = save_validator_round_disk_cache(path, &snapshot)
            {
                eprintln!("failed to save validator round cache: {error:#}");
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
}

async fn scan_validator_election_round_history(
    rpc: &JrpcTransport,
    account: &str,
    scan: ValidatorRoundHistoryScan<'_>,
) -> Result<ValidatorRoundData> {
    let scan_start_at = scan
        .stake_at
        .saturating_sub(scan.elections_end_before)
        .saturating_add(60);
    let scan_stop_before = scan
        .stake_at
        .saturating_sub(scan.elections_start_before)
        .saturating_sub(60);
    let mut continuation = estimated_transaction_lt_at(rpc, account, scan_start_at).await?;
    let mut history = HashMap::new();

    for page in 0..700 {
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

        let mut next_continuation = None;
        let mut reached_stop = false;
        for tx_boc in tx_bocs {
            let transaction: Transaction = BocRepr::decode_base64(tx_boc)?;
            next_continuation = Some(transaction.prev_trans_lt.to_string());
            if transaction.now < scan_stop_before {
                reached_stop = true;
            }

            let Some(submission) = parse_election_submission(&transaction, scan.election_fee)?
            else {
                continue;
            };
            if scan.debug_history {
                eprintln!(
                    "election tx: round={} page={} now={} stake_at={} pubkey={} wallet={} stake={}",
                    scan.stake_at,
                    page,
                    transaction.now,
                    submission.stake_at,
                    submission.public_key,
                    submission.wallet,
                    submission.stake
                );
            }
            if submission.stake_at != scan.stake_at
                || !scan.target_keys.contains(&submission.public_key)
            {
                continue;
            }

            history
                .entry(submission.public_key)
                .or_insert(ValidatorElectionHistory {
                    wallet: submission.wallet,
                    stake: submission.stake,
                    reward: None,
                });
        }

        if scan.debug_history {
            eprintln!(
                "history scan round={} page={} mapped={}/{} continuation={:?}",
                scan.stake_at,
                page,
                history.len(),
                scan.target_keys.len(),
                next_continuation
            );
        }
        if history.len() >= scan.target_keys.len() || reached_stop {
            break;
        }
        if next_continuation.as_deref() == Some("0") || next_continuation == continuation {
            break;
        }
        continuation = next_continuation;
    }

    Ok(ValidatorRoundData {
        validators: history,
        total_stake: None,
        total_stake_raw: None,
        total_reward: None,
        total_reward_raw: None,
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

fn load_validator_round_disk_cache(path: &Path) -> Result<HashMap<String, ValidatorRoundData>> {
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
    fs::write(path, content).with_context(|| format!("failed to write {}", path.display()))
}

fn load_config(path: Option<&PathBuf>) -> Result<AppConfig> {
    let content = match path {
        Some(path) => fs::read_to_string(path)
            .with_context(|| format!("failed to read config {}", path.display()))?,
        None => {
            fs::read_to_string("validators_clock.json").unwrap_or_else(|_| DEFAULT_CONFIG.into())
        }
    };

    serde_json::from_str(&content).context("failed to parse validators clock config")
}

fn endpoint_label(endpoint: &str) -> String {
    endpoint
        .trim()
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_end_matches('/')
        .to_owned()
}

fn default_listen() -> String {
    "127.0.0.1:8787".to_owned()
}

fn default_refresh_seconds() -> u64 {
    60
}

fn default_cache_path() -> PathBuf {
    PathBuf::from("validators_clock_cache.json")
}

fn default_chain_color() -> String {
    "#38bdf8".to_owned()
}

fn default_token_symbol() -> String {
    "TOKENS".to_owned()
}

fn now_sec() -> Result<u64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system time is before UNIX epoch")?
        .as_secs())
}

fn json_response<T: Serialize>(value: &T) -> HttpResponse {
    match serde_json::to_vec(value) {
        Ok(body) => HttpResponse::owned(200, "application/json; charset=utf-8", body),
        Err(error) => json_error(500, &error.to_string()),
    }
}

fn json_error(status: u16, message: &str) -> HttpResponse {
    let body = serde_json::json!({ "error": message });
    HttpResponse::owned(
        status,
        "application/json; charset=utf-8",
        serde_json::to_vec(&body).unwrap_or_else(|_| b"{\"error\":\"internal error\"}".to_vec()),
    )
}

struct HttpResponse {
    status: u16,
    content_type: &'static str,
    body: Vec<u8>,
}

impl HttpResponse {
    fn ok(content_type: &'static str, body: &[u8]) -> Self {
        Self::owned(200, content_type, body.to_vec())
    }

    fn owned(status: u16, content_type: &'static str, body: Vec<u8>) -> Self {
        Self {
            status,
            content_type,
            body,
        }
    }

    fn empty(status: u16) -> Self {
        Self::owned(status, "text/plain; charset=utf-8", Vec::new())
    }

    fn without_body(mut self) -> Self {
        self.body.clear();
        self
    }
}

async fn write_response(stream: &mut TcpStream, response: HttpResponse) -> Result<()> {
    let reason = reason_phrase(response.status);
    let headers = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        response.status,
        reason,
        response.content_type,
        response.body.len()
    );
    stream.write_all(headers.as_bytes()).await?;
    stream.write_all(&response.body).await?;
    stream.shutdown().await?;
    Ok(())
}

fn reason_phrase(status: u16) -> &'static str {
    match status {
        200 => "OK",
        204 => "No Content",
        404 => "Not Found",
        405 => "Method Not Allowed",
        500 => "Internal Server Error",
        _ => "OK",
    }
}
