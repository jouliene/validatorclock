//! Finding out where the validators of a chain are running.
//!
//! The chain says who validates; it does not say where. Only the network's
//! DHT knows that, and until now a second program was asking it and leaving
//! the answer in a file for this one to read. That program is what this
//! module replaces: the same question, asked from inside the process that
//! needs the answer.
//!
//! The answer is still written to a file, and deliberately so. It is what the
//! node location map reads, and it is what lets a restart put addresses on the
//! page immediately instead of waiting for a DHT that takes a minute to warm
//! up. Restarts are frequent while the site is being worked on; the map should
//! not go blank for each one.

mod dht;
mod memory;
mod tycho;

use crate::chain::ClockSnapshot;
use crate::config::{NodeResolverChainConfig, NodeResolverConfig, ResolverProtocol};
use crate::fsutil::write_file_atomic;
use crate::state::AppState;
use anyhow::{Context, Result, anyhow};
use dht::{AdnlDhtResolver, Resolution, is_hex_32};
use futures::{StreamExt, stream};
use memory::ResolvedAddressMemory;
use serde::Serialize;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time::sleep;
use tracing::{debug, error, info, warn};
use tycho::TychoDhtResolver;

/// How long to wait before building the DHT node again after it failed. The
/// usual reasons - the address already taken, a bootstrap file not yet in
/// place - are the kind that a person fixes, so this is unhurried.
const RETRY_AFTER: Duration = Duration::from_secs(60);
/// How long to wait for the chain to produce its first snapshot. Nothing can
/// be resolved before there is a validator set, and on a cold start that set
/// arrives seconds after this thread does - so this waits in short steps
/// rather than standing down for a whole refresh interval.
const SNAPSHOT_POLL: Duration = Duration::from_secs(5);
/// How many times the addresses a pass could not reach are asked about again
/// once the pass is over.
///
/// A sweep of a whole validator set loses a handful of addresses that answer
/// perfectly well when asked for on their own. Asking again is worth doing;
/// asking again *through the same DHT client* is not, and the difference
/// between the two is the whole of this. See `recover_misses`.
const RECOVERY_ROUNDS: usize = 2;
/// How long to let the network settle before asking again.
const RECOVERY_PAUSE: Duration = Duration::from_secs(20);
/// How many of those second asks go out at once. Few, on purpose: a fresh
/// client's short candidate list is the thing being protected here.
const RECOVERY_WORKERS: usize = 2;
/// How many times to ask the system for a port before giving the round up. A
/// port free a moment ago can be taken by the time the socket opens.
const BIND_TRIES: usize = 3;
/// How long one round of second asks may run. A round normally asks about
/// four to eight addresses and is done in well under this; the budget is here
/// so that a pass which has lost a great many of them still ends in time for
/// the next one.
const RECOVERY_ROUND_BUDGET: Duration = Duration::from_secs(90);
const SCHEMA_VERSION: u32 = 1;

/// Start one collector per configured chain.
///
/// Each gets an operating system thread of its own with a single-threaded
/// runtime on it. That is not a preference: the DHT stack holds values that
/// cannot be sent between threads, so it cannot live in a task the scheduler
/// moves around. Giving it a thread also keeps its work off the threads that
/// answer requests, which is the right place for something that spends its
/// time waiting on strangers.
pub(crate) fn spawn_background_refresh(state: Arc<AppState>) {
    let config = &state.config.node_resolver;
    for (chain_id, _) in config.active_chains() {
        let chain_id = chain_id.to_owned();
        let state = Arc::clone(&state);
        let thread_name = format!("node-resolver-{chain_id}");
        let thread_chain_id = chain_id.clone();
        let spawned = std::thread::Builder::new()
            .name(thread_name.clone())
            .spawn(move || {
                let runtime = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(runtime) => runtime,
                    Err(error) => {
                        error!(chain_id, error = ?error, "node resolver could not start a runtime");
                        return;
                    }
                };
                // A panic in the DHT stack ends this thread and nothing
                // else. Catching it here means the collector comes back
                // instead of staying dead until someone restarts the site.
                loop {
                    let state = Arc::clone(&state);
                    let chain_id = chain_id.clone();
                    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        runtime.block_on(supervise_chain(state, chain_id))
                    }));
                    match outcome {
                        Ok(()) => return,
                        Err(_) => {
                            error!(
                                chain_id = %thread_chain_id,
                                retry_seconds = RETRY_AFTER.as_secs(),
                                "node resolver panicked; restarting it"
                            );
                            std::thread::sleep(RETRY_AFTER);
                        }
                    }
                }
            });
        if let Err(error) = spawned {
            error!(thread_name, error = ?error, "node resolver thread could not be started");
        }
    }
}

/// Keep one chain's resolver running.
///
/// The DHT stack is third-party code parsing what strangers send it, and this
/// runs in the same process as the site. A failure here must cost the map its
/// freshness and nothing else, so every error is caught, logged and retried
/// rather than allowed to end the task for good.
async fn supervise_chain(state: Arc<AppState>, chain_id: String) {
    let config = &state.config.node_resolver;
    sleep(Duration::from_secs(config.startup_delay_seconds)).await;

    loop {
        match run_chain(&state, &chain_id).await {
            Ok(()) => {
                // The loop inside only returns when the chain is no longer
                // configured for this, which is not an error and not a reason
                // to keep trying.
                info!(chain_id, "node resolver stopped");
                return;
            }
            Err(error) => {
                error!(
                    chain_id,
                    error = ?error,
                    retry_seconds = RETRY_AFTER.as_secs(),
                    "node resolver failed; retrying"
                );
                sleep(RETRY_AFTER).await;
            }
        }
    }
}

async fn run_chain(state: &AppState, chain_id: &str) -> Result<()> {
    let config = &state.config.node_resolver;
    let Some(chain) = config.chains.get(chain_id).filter(|chain| chain.enabled) else {
        return Ok(());
    };
    // Built once and kept: the socket, the bootstrap peers and everything the
    // DHT has learned are worth far more than they cost to hold, and throwing
    // them away every cycle would make each round start from a cold network.
    let resolver = ChainResolver::open(chain, config, config.local_addr_for(chain))
        .await
        .context("failed to start the DHT resolver")?;

    // What the last run learned, so a restart does not begin by forgetting.
    let output_path = chain
        .output_path
        .as_deref()
        .ok_or_else(|| anyhow!("output_path is required"))?;
    let mut memory = ResolvedAddressMemory::from_previous_output(output_path);

    info!(
        chain_id,
        local_addr = %resolver.local_addr(),
        bootstrap_nodes = resolver.bootstrap_nodes(),
        refresh_seconds = config.refresh_seconds,
        remembered = memory.len(),
        "node resolver started"
    );

    loop {
        let snapshot = wait_for_snapshot(state, chain_id).await;
        if let Err(error) =
            collect_once(chain_id, chain, config, &resolver, snapshot, &mut memory).await
        {
            warn!(chain_id, error = ?error, "node resolver pass failed");
        }
        sleep(Duration::from_secs(config.refresh_seconds)).await;
    }
}

/// Wait until the chain has a validator set to work from.
async fn wait_for_snapshot(state: &AppState, chain_id: &str) -> Arc<ClockSnapshot> {
    let mut waited = Duration::ZERO;
    loop {
        if let Some(snapshot) = state.cached_snapshot(chain_id).await {
            return snapshot;
        }
        if waited.is_zero() {
            debug!(chain_id, "waiting for the first validator set");
        }
        sleep(SNAPSHOT_POLL).await;
        waited += SNAPSHOT_POLL;
    }
}

async fn collect_once(
    chain_id: &str,
    chain: &NodeResolverChainConfig,
    config: &NodeResolverConfig,
    resolver: &ChainResolver,
    // The validator set this process already holds. The program this replaces
    // asked the site's own HTTP API for it, which meant it could not run while
    // the site was down and fetched over the network what was already in
    // memory a function call away.
    snapshot: Arc<ClockSnapshot>,
    memory: &mut ResolvedAddressMemory,
) -> Result<()> {
    resolver.warmup(chain_id).await;

    let now = crate::timeutil::now_sec();
    let validators = &snapshot.current_set.validators;
    let resolved = stream::iter(validators.iter())
        .map(|validator| async move {
            let resolution = match address_to_look_up(validator) {
                Ok(adnl_addr) => resolver.resolve(adnl_addr, now).await,
                Err(resolution) => resolution,
            };
            ResolvedValidator {
                validator_public_key: validator.public_key.clone(),
                adnl_addr: validator.adnl_addr.clone(),
                wallet: validator.wallet.clone(),
                source_address: validator.source.as_ref().map(|s| s.address.clone()),
                source_contract_type_hash: validator
                    .source
                    .as_ref()
                    .and_then(|s| s.contract_type_hash.clone()),
                contract_type: validator.contract_type.clone(),
                stake: validator.stake.clone(),
                weight: Some(validator.weight.clone()),
                resolution,
            }
        })
        .buffer_unordered(config.workers.max(1))
        .collect::<Vec<_>>()
        .await;

    let mut resolved = resolved;
    let recovered_total = recover_misses(chain_id, chain, config, &mut resolved).await;

    apply_remembered_addresses(&mut resolved, memory, now);
    memory.retain_only(
        &validators
            .iter()
            .filter_map(|validator| validator.adnl_addr.clone())
            .collect::<Vec<_>>(),
    );
    let totals = PassTotals::of(&resolved);

    let output_path = chain
        .output_path
        .as_deref()
        .ok_or_else(|| anyhow!("output_path is required"))?;
    let output = ResolvedSet {
        schema_version: SCHEMA_VERSION,
        chain_id: chain_id.to_owned(),
        fetched_at: snapshot.fetched_at,
        generated_at: crate::timeutil::now_sec(),
        round_id: snapshot.current_set.round_id,
        validators_total: validators.len(),
        validators_main: usize::from(snapshot.current_set.main),
        validators_with_adnl: totals.with_adnl,
        resolved_total: totals.resolved,
        remembered_total: totals.remembered,
        placed_total: totals.placed,
        resolver: ResolverMetadata {
            local_addr: resolver.local_addr().to_owned(),
            bootstrap_nodes: resolver.bootstrap_nodes(),
        },
        validators: resolved,
    };

    let body =
        serde_json::to_vec_pretty(&output).context("failed to serialize the resolved set")?;
    write_file_atomic(output_path, &body, 0o644)
        .with_context(|| format!("failed to write {}", output_path.display()))?;

    info!(
        chain_id,
        validators_total = output.validators_total,
        validators_with_adnl = totals.with_adnl,
        resolved_total = totals.resolved,
        recovered_total,
        remembered_total = totals.remembered,
        placed_total = totals.placed,
        round_id = output.round_id,
        output_path = %output_path.display(),
        "resolved validator addresses"
    );
    Ok(())
}

/// Where a chain's addresses are looked up.
///
/// Everscale and TON publish theirs in an ADNL DHT. Tycho has no ADNL at all -
/// its peers are QUIC endpoints and their addresses live in a DHT of its own -
/// and the difference ends at the lookup. A pass, the second ask after it and
/// the memory behind both are the same work whichever network answered, so
/// only this one thing is told apart.
enum ChainResolver {
    Adnl(AdnlDhtResolver),
    Tycho(TychoDhtResolver),
}

impl ChainResolver {
    async fn open(
        chain: &NodeResolverChainConfig,
        config: &NodeResolverConfig,
        local_addr: &str,
    ) -> Result<Self> {
        let global_config_path = chain
            .global_config_path
            .as_deref()
            .ok_or_else(|| anyhow!("global_config_path is required"))?;
        let lookup_timeout = Duration::from_secs(config.lookup_timeout_seconds);

        Ok(match chain.protocol {
            ResolverProtocol::Adnl => Self::Adnl(
                AdnlDhtResolver::new(global_config_path, local_addr, lookup_timeout).await?,
            ),
            ResolverProtocol::Tycho => Self::Tycho(
                TychoDhtResolver::new(global_config_path, local_addr, lookup_timeout).await?,
            ),
        })
    }

    fn local_addr(&self) -> &str {
        match self {
            Self::Adnl(resolver) => resolver.local_addr(),
            Self::Tycho(resolver) => resolver.local_addr(),
        }
    }

    fn bootstrap_nodes(&self) -> usize {
        match self {
            Self::Adnl(resolver) => resolver.bootstrap_nodes(),
            Self::Tycho(resolver) => resolver.bootstrap_nodes(),
        }
    }

    /// Reach the network before the pass, and say what came of it. The two
    /// stacks warm up differently enough that each reports its own way.
    async fn warmup(&self, chain_id: &str) {
        match self {
            Self::Adnl(resolver) => {
                let warmup = resolver.warmup_network().await;
                debug!(
                    chain_id,
                    checked = warmup.checked,
                    responsive = warmup.responsive,
                    errors = warmup.errors,
                    known_nodes = warmup.known_nodes,
                    "DHT warmed up"
                );
            }
            Self::Tycho(resolver) => {
                resolver.warmup_network().await;
                debug!(
                    chain_id,
                    bootstrap_nodes = resolver.bootstrap_nodes(),
                    "Tycho DHT warmed up"
                );
            }
        }
    }

    async fn resolve(&self, adnl_addr: &str, now: u64) -> Resolution {
        match self {
            Self::Adnl(resolver) => resolver.resolve(adnl_addr, now).await,
            Self::Tycho(resolver) => resolver.resolve(adnl_addr, now).await,
        }
    }

    /// Close the sockets a throwaway resolver opened. The ADNL node has to be
    /// told; the Tycho one ends when it is dropped, because the tasks behind
    /// it hold nothing but a weak reference to it.
    async fn shutdown(self) {
        match self {
            Self::Adnl(resolver) => resolver.shutdown().await,
            Self::Tycho(_) => {}
        }
    }
}

/// Ask again about the validators this pass lost - through a DHT client that
/// has not just been through a sweep.
///
/// A client that has looked up four hundred addresses is not the client that
/// started. The library keeps at most a few candidate peers per key and scores
/// down every peer that fails to answer, so by the end of a sweep the peers
/// nearest some keys are all in its bad books. A search like that does not
/// time out - it reports "not found" in half a second, having asked no one.
///
/// Measured on one set of six misses, asked again at the same moment: through
/// the client that lost them, none came back; through a client built there and
/// then, three did - one of them an address that a whole afternoon of passes,
/// and the resolver this project replaces, had never once found.
///
/// So each round gets a socket and a routing table of its own and throws them
/// away afterwards. It costs a greeting to the bootstrap peers and a handful
/// of lookups.
async fn recover_misses(
    chain_id: &str,
    chain: &NodeResolverChainConfig,
    config: &NodeResolverConfig,
    resolved: &mut [ResolvedValidator],
) -> usize {
    let mut recovered_total = 0;
    for round in 1..=RECOVERY_ROUNDS {
        let misses = misses_worth_asking_again(resolved);
        if misses.is_empty() {
            break;
        }

        sleep(RECOVERY_PAUSE).await;
        let resolver = match unused_resolver(chain, config).await {
            Ok(resolver) => resolver,
            Err(error) => {
                debug!(chain_id, error = ?error, "no second resolver for the second ask");
                break;
            }
        };
        resolver.warmup(chain_id).await;

        let asked = misses.len();
        let deadline = Instant::now() + RECOVERY_ROUND_BUDGET;
        let mut answers = stream::iter(misses)
            .map(|(index, adnl_addr)| {
                let resolver = &resolver;
                // Dated when this lookup was made, not when the round began:
                // a round can take a minute and a half, and the map measures
                // how old a point is from exactly this.
                async move {
                    let now = crate::timeutil::now_sec();
                    (index, resolver.resolve(&adnl_addr, now).await)
                }
            })
            .buffer_unordered(RECOVERY_WORKERS);

        // Taken one at a time rather than collected, so that a round which
        // runs long can be stopped while keeping what it has already found.
        // The usual round asks about a handful; a bad enough moment on the
        // network could hand it a hundred, and the pass still has to end.
        let mut recovered = 0;
        let mut answered = 0;
        while let Some((index, resolution)) = answers.next().await {
            answered += 1;
            if resolution.is_resolved() {
                recovered += 1;
                resolved[index].resolution = resolution;
            }
            if Instant::now() >= deadline {
                break;
            }
        }
        drop(answers);
        recovered_total += recovered;
        debug!(
            chain_id,
            round, asked, answered, recovered, "asked again about the addresses this pass lost"
        );
        resolver.shutdown().await;
    }
    recovered_total
}

/// A DHT client of its own for one round of second asks.
///
/// The chain's configured port belongs to the resolver that runs the sweeps
/// and cannot be shared - two sockets on one address means the second one
/// never opens - so this one takes a free port, and takes it by name.
async fn unused_resolver(
    chain: &NodeResolverChainConfig,
    config: &NodeResolverConfig,
) -> Result<ChainResolver> {
    let host = host_of(config.local_addr_for(chain));

    let mut last_error = None;
    for _ in 0..BIND_TRIES {
        let port = free_port(host)?;
        match ChainResolver::open(chain, config, &format!("{host}:{port}")).await {
            Ok(resolver) => return Ok(resolver),
            // Between asking the system for a free port and opening the socket
            // on it, someone else can have taken it. Ask for another.
            Err(error) => last_error = Some(error),
        }
    }
    Err(last_error.unwrap_or_else(|| anyhow!("no free port for a second resolver")))
}

/// A port to open the second ask's socket on.
///
/// Not port zero, which is how one usually says "any free port" and is the
/// wrong way to say it here: the ADNL node announces to every peer it greets
/// the address to find it at, and a node announcing port zero is one a good
/// share of the network will not deal with. Measured over full sweeps from
/// the same machine, minutes apart: 386 of 393 resolved from a named port,
/// 330 from port zero - and fewer bootstrap peers answered the greeting, too.
/// So the port is borrowed from the system first and then announced honestly.
fn free_port(host: &str) -> Result<u16> {
    let socket = std::net::UdpSocket::bind((host, 0))
        .with_context(|| format!("failed to find a free port on {host}"))?;
    Ok(socket.local_addr()?.port())
}

/// The interface part of a configured `host:port`.
fn host_of(configured: &str) -> &str {
    match configured.rsplit_once(':') {
        Some((host, _)) => host,
        None => configured,
    }
}

/// The address to ask the DHT about, or what to write down instead.
///
/// A validator the chain named no address for, or named a malformed one, is
/// not a lookup that failed: it is one that was never possible. The file says
/// which, and the second ask knows not to spend a lookup on it.
fn address_to_look_up(validator: &crate::chain::ValidatorDto) -> Result<&str, Resolution> {
    match validator.adnl_addr.as_deref() {
        None => Err(Resolution::missing_adnl()),
        Some(adnl_addr) if !is_hex_32(adnl_addr) => Err(Resolution::invalid_adnl(adnl_addr)),
        Some(adnl_addr) => Ok(adnl_addr),
    }
}

/// Keep the addresses this pass confirmed, and offer back the ones it could
/// not reach.
///
/// A lookup that failed has not said the address is gone, only that this pass
/// could not reach it - and the passes disagree: of ten that failed one pass,
/// five answered the next. An address confirmed within the hour is offered
/// again rather than dropped, marked as remembered and carrying the time it
/// was last confirmed.
fn apply_remembered_addresses(
    resolved: &mut [ResolvedValidator],
    memory: &mut ResolvedAddressMemory,
    now: u64,
) {
    for validator in resolved {
        match (&validator.adnl_addr, validator.resolution.is_resolved()) {
            (Some(adnl_addr), true) => {
                if let Some(address) = validator.resolution.addresses.first() {
                    memory.remember(adnl_addr, address, now);
                }
            }
            (Some(adnl_addr), false) => {
                if let Some(recalled) = memory.recall(adnl_addr, now) {
                    validator.resolution = recalled;
                }
            }
            (None, _) => {}
        }
    }
}

/// What one pass came to.
struct PassTotals {
    /// Confirmed by the DHT during this pass.
    resolved: usize,
    /// Not reached this pass, and offered from memory instead.
    remembered: usize,
    /// With an address to put on the map at all, however it was arrived at.
    placed: usize,
    /// Named an address by the chain, whether or not it answered.
    with_adnl: usize,
}

impl PassTotals {
    fn of(resolved: &[ResolvedValidator]) -> Self {
        Self {
            resolved: resolved
                .iter()
                .filter(|validator| validator.resolution.is_resolved())
                .count(),
            remembered: resolved
                .iter()
                .filter(|validator| validator.resolution.status == "remembered")
                .count(),
            placed: resolved
                .iter()
                .filter(|validator| validator.resolution.has_address())
                .count(),
            with_adnl: resolved
                .iter()
                .filter(|validator| validator.adnl_addr.is_some())
                .count(),
        }
    }
}

/// Which validators are worth a second ask, and their addresses.
///
/// Only the ones whose lookup went out and came back empty. A validator the
/// chain gave no ADNL address for, or gave a malformed one, has nothing to ask
/// about - asking again would spend a lookup to be told the same thing.
fn misses_worth_asking_again(resolved: &[ResolvedValidator]) -> Vec<(usize, String)> {
    resolved
        .iter()
        .enumerate()
        .filter(|(_, validator)| validator.resolution.is_failed())
        .filter_map(|(index, validator)| {
            validator
                .adnl_addr
                .clone()
                .map(|adnl_addr| (index, adnl_addr))
        })
        .collect()
}

/// What one pass produced. The shape is the one the node location map already
/// reads, so nothing downstream has to learn a new file.
#[derive(Debug, Serialize)]
struct ResolvedSet {
    schema_version: u32,
    chain_id: String,
    fetched_at: u64,
    generated_at: u64,
    round_id: u32,
    validators_total: usize,
    validators_main: usize,
    validators_with_adnl: usize,
    /// Confirmed by the DHT during this pass.
    resolved_total: usize,
    /// Not reached this pass, but confirmed within the hour and offered again.
    remembered_total: usize,
    /// How many validators have an address to put on the map at all.
    placed_total: usize,
    resolver: ResolverMetadata,
    validators: Vec<ResolvedValidator>,
}

#[derive(Debug, Serialize)]
struct ResolverMetadata {
    local_addr: String,
    bootstrap_nodes: usize,
}

#[derive(Debug, Serialize)]
struct ResolvedValidator {
    validator_public_key: String,
    adnl_addr: Option<String>,
    wallet: Option<String>,
    source_address: Option<String>,
    source_contract_type_hash: Option<String>,
    contract_type: Option<String>,
    stake: Option<String>,
    weight: Option<String>,
    resolution: Resolution,
}

#[cfg(test)]
mod tests;
