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

use crate::chain::ClockSnapshot;
use crate::config::{NodeResolverChainConfig, NodeResolverConfig};
use crate::fsutil::write_file_atomic;
use crate::state::AppState;
use anyhow::{Context, Result, anyhow};
use dht::{AdnlDhtResolver, Resolution, is_hex_32};
use futures::{StreamExt, stream};
use memory::ResolvedAddressMemory;
use serde::Serialize;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, error, info, warn};

/// How long to wait before building the DHT node again after it failed. The
/// usual reasons - the address already taken, a bootstrap file not yet in
/// place - are the kind that a person fixes, so this is unhurried.
const RETRY_AFTER: Duration = Duration::from_secs(60);
/// How long to wait for the chain to produce its first snapshot. Nothing can
/// be resolved before there is a validator set, and on a cold start that set
/// arrives seconds after this thread does - so this waits in short steps
/// rather than standing down for a whole refresh interval.
const SNAPSHOT_POLL: Duration = Duration::from_secs(5);
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
    let global_config_path = chain
        .global_config_path
        .as_deref()
        .ok_or_else(|| anyhow!("global_config_path is required"))?;

    // Built once and kept: the socket, the bootstrap peers and everything the
    // DHT has learned are worth far more than they cost to hold, and throwing
    // them away every cycle would make each round start from a cold network.
    let resolver = AdnlDhtResolver::new(
        global_config_path,
        config.local_adnl_addr_for(chain),
        Duration::from_secs(config.lookup_timeout_seconds),
    )
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
        local_adnl_addr = %resolver.local_adnl_addr(),
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
async fn wait_for_snapshot(state: &AppState, chain_id: &str) -> ClockSnapshot {
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
    resolver: &AdnlDhtResolver,
    // The validator set this process already holds. The program this replaces
    // asked the site's own HTTP API for it, which meant it could not run while
    // the site was down and fetched over the network what was already in
    // memory a function call away.
    snapshot: ClockSnapshot,
    memory: &mut ResolvedAddressMemory,
) -> Result<()> {
    let warmup = resolver.warmup_network().await;
    debug!(
        chain_id,
        checked = warmup.checked,
        responsive = warmup.responsive,
        errors = warmup.errors,
        known_nodes = warmup.known_nodes,
        "DHT warmed up"
    );

    let now = crate::timeutil::now_sec();
    let validators = &snapshot.current_set.validators;
    let resolved = stream::iter(validators.iter())
        .map(|validator| async move {
            let resolution = match validator.adnl_addr.as_deref() {
                None => Resolution::missing_adnl(),
                Some(adnl_addr) if !is_hex_32(adnl_addr) => Resolution::invalid_adnl(adnl_addr),
                Some(adnl_addr) => resolver.resolve(adnl_addr, now).await,
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

    // A lookup that timed out has not told us the address is gone, only that
    // this pass could not reach it - and the passes disagree: of ten that
    // failed one pass, five answered the next. An address the DHT confirmed
    // within the hour is offered again rather than dropped, marked as
    // remembered and carrying the time it was last confirmed.
    let mut resolved = resolved;
    for validator in &mut resolved {
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
    memory.retain_only(
        &validators
            .iter()
            .filter_map(|validator| validator.adnl_addr.clone())
            .collect::<Vec<_>>(),
    );

    let resolved_total = resolved
        .iter()
        .filter(|validator| validator.resolution.is_resolved())
        .count();
    let remembered_total = resolved
        .iter()
        .filter(|validator| validator.resolution.status == "remembered")
        .count();
    let placed_total = resolved
        .iter()
        .filter(|validator| validator.resolution.has_address())
        .count();
    let with_adnl = resolved
        .iter()
        .filter(|validator| validator.adnl_addr.is_some())
        .count();

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
        validators_with_adnl: with_adnl,
        resolved_total,
        remembered_total,
        placed_total,
        resolver: ResolverMetadata {
            local_adnl_addr: resolver.local_adnl_addr().to_owned(),
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
        validators_with_adnl = with_adnl,
        resolved_total,
        remembered_total,
        placed_total,
        round_id = output.round_id,
        output_path = %output_path.display(),
        "resolved validator addresses"
    );
    Ok(())
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
    local_adnl_addr: String,
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
