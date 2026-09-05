mod election;
mod frozen;
mod graphql;
mod snapshot;
mod toncenter;
mod toncenter_stack;

use super::dto::RoundStatsPointDto;
use super::graphql_client::is_graphql_endpoint;
use super::round_stats::build_round_stats_response;
use super::util::{endpoint_label, now_sec};
use super::{ChainRoundStatsDto, ClockSnapshot};
use crate::config::ChainConfig;
use anyhow::{Context, Result, anyhow};
use election::fetch_election;
use frozen::fetch_frozen_validator_round_data;
use minik2::{Config, Transport, ValidatorSet};
use std::env;
use tracing::{debug, info, warn};

const STALE_SNAPSHOT_GRACE_SECONDS: u64 = 300;
pub(crate) async fn fetch_chain_snapshot(chain: &ChainConfig) -> Result<ClockSnapshot> {
    match fetch_chain_snapshot_from_endpoint(chain, &chain.rpc).await {
        Ok(mut snapshot) => {
            if let Some(stale_reason) = snapshot_stale_reason(chain, &snapshot) {
                let primary_reason = format!("appears stale: {stale_reason}");
                match fetch_fallback_snapshot(chain, &primary_reason, true).await {
                    Ok(snapshot) => return Ok(snapshot),
                    Err(error) => {
                        // Which endpoint is behind is the operator's business.
                        // The reader is told what it means for what they are
                        // looking at - the numbers are behind - and nothing
                        // about the machinery that serves them.
                        warn!(
                            chain_id = %chain.id,
                            endpoint = %super::util::endpoint_label(&chain.rpc),
                            reason = %stale_reason,
                            error = %error,
                            "primary RPC is stale and no fallback answered"
                        );
                        set_snapshot_warning(&mut snapshot, stale_data_warning(&stale_reason));
                    }
                }
            }

            Ok(snapshot)
        }
        Err(primary_error) => {
            if chain.rpc_fallbacks.is_empty() {
                return Err(primary_error);
            }

            let primary_error = primary_error.to_string();
            fetch_fallback_snapshot(chain, &format!("failed: {primary_error}"), false)
                .await
                .map_err(|fallback_error| {
                    anyhow!("primary RPC failed: {}; {}", primary_error, fallback_error)
                })
        }
    }
}

pub(crate) async fn fetch_chain_round_stats(
    chain: &ChainConfig,
    history_points: Vec<RoundStatsPointDto>,
) -> Result<ChainRoundStatsDto> {
    let mut errors = Vec::new();
    for endpoint in std::iter::once(&chain.rpc).chain(chain.rpc_fallbacks.iter()) {
        match fetch_chain_round_stats_from_endpoint(chain, endpoint, &history_points).await {
            Ok(stats) => return Ok(stats),
            Err(error) => errors.push(format!("{}: {}", endpoint_label(endpoint), error)),
        }
    }

    Err(anyhow!("round stats fetch failed: {}", errors.join("; ")))
}

async fn fetch_fallback_snapshot(
    chain: &ChainConfig,
    primary_reason: &str,
    require_fresh: bool,
) -> Result<ClockSnapshot> {
    if chain.rpc_fallbacks.is_empty() {
        return Err(anyhow!("no fallback RPCs configured"));
    }

    let mut fallback_errors = Vec::new();
    for fallback in &chain.rpc_fallbacks {
        match fetch_chain_snapshot_from_endpoint(chain, fallback).await {
            Ok(mut snapshot) => {
                // Which endpoint answered is worth a log line and nothing more:
                // a reader looking at a validator clock has no use for it, and
                // an internal address is not theirs to see.
                info!(
                    chain_id = %chain.id,
                    endpoint = %super::util::endpoint_label(fallback),
                    primary = %super::util::endpoint_label(&chain.rpc),
                    reason = %primary_reason,
                    "answering from a fallback RPC"
                );

                if let Some(stale_reason) = snapshot_stale_reason(chain, &snapshot) {
                    if require_fresh {
                        fallback_errors.push(format!(
                            "{} returned stale snapshot: {}",
                            super::util::endpoint_label(fallback),
                            stale_reason
                        ));
                        continue;
                    }

                    warn!(
                        chain_id = %chain.id,
                        endpoint = %super::util::endpoint_label(fallback),
                        reason = %stale_reason,
                        "fallback RPC answered with a stale snapshot"
                    );
                    set_snapshot_warning(&mut snapshot, stale_data_warning(&stale_reason));
                }
                return Ok(snapshot);
            }
            Err(error) => {
                fallback_errors.push(format!(
                    "{}: {}",
                    super::util::endpoint_label(fallback),
                    error
                ));
            }
        }
    }

    Err(anyhow!(
        "fallback RPCs failed: {}",
        fallback_errors.join("; ")
    ))
}

async fn fetch_chain_snapshot_from_endpoint(
    chain: &ChainConfig,
    rpc: &str,
) -> Result<ClockSnapshot> {
    if toncenter::is_toncenter_endpoint(rpc) {
        return toncenter::fetch_chain_snapshot(chain, rpc).await;
    }
    if is_graphql_endpoint(rpc) {
        return graphql::fetch_chain_snapshot(chain, rpc).await;
    }

    fetch_chain_snapshot_from_jrpc(chain, rpc).await
}

async fn fetch_chain_round_stats_from_endpoint(
    chain: &ChainConfig,
    rpc: &str,
    history_points: &[RoundStatsPointDto],
) -> Result<ChainRoundStatsDto> {
    if toncenter::is_toncenter_endpoint(rpc) {
        return toncenter::fetch_chain_round_stats(chain, rpc, history_points).await;
    }
    if is_graphql_endpoint(rpc) {
        return graphql::fetch_chain_round_stats(chain, rpc, history_points).await;
    }

    fetch_chain_round_stats_from_jrpc(chain, rpc, history_points).await
}

async fn fetch_chain_snapshot_from_jrpc(chain: &ChainConfig, rpc: &str) -> Result<ClockSnapshot> {
    let transport =
        Transport::jrpc(rpc).with_context(|| format!("invalid RPC endpoint for `{}`", chain.id))?;
    let config = Config::fetch(&transport)
        .await
        .with_context(|| format!("failed to fetch config from `{}`", chain.id))?;
    let timings = config.election_timings()?;
    let observed_at = now_sec()?;
    let (current_set, next_set) = effective_validator_sets(
        config.current_validator_set()?,
        config.next_validator_set()?,
        observed_at,
    );
    let election = fetch_election(&transport, &config)
        .await
        .unwrap_or_default();
    // Live refreshes only use elector/full-round state so history can prove both
    // participation and absence for recorded rounds.
    let validator_round_data_result = fetch_frozen_validator_round_data(&transport, &config).await;
    let validator_round_data = match validator_round_data_result {
        Ok(round_data) => round_data,
        Err(error) => {
            if env::var_os("VALIDATORCLOCK_DEBUG_HISTORY").is_some() {
                debug!(error = ?error, "validator round data failed");
            }
            Default::default()
        }
    };

    Ok(snapshot::assemble_snapshot(snapshot::SnapshotParts {
        chain,
        endpoint: rpc,
        observed_at,
        global_id: config.global_id(),
        seqno: config.seqno(),
        timings,
        current_set,
        next_set,
        election,
        validator_round_data,
    }))
}

async fn fetch_chain_round_stats_from_jrpc(
    chain: &ChainConfig,
    rpc: &str,
    history_points: &[RoundStatsPointDto],
) -> Result<ChainRoundStatsDto> {
    let transport =
        Transport::jrpc(rpc).with_context(|| format!("invalid RPC endpoint for `{}`", chain.id))?;
    let config = Config::fetch(&transport)
        .await
        .with_context(|| format!("failed to fetch config from `{}`", chain.id))?;
    let timings = config.election_timings()?;
    let observed_at = now_sec()?;
    let (current_set, _) = effective_validator_sets(
        config.current_validator_set()?,
        config.next_validator_set()?,
        observed_at,
    );
    let validator_round_data = fetch_frozen_validator_round_data(&transport, &config).await?;

    Ok(build_round_stats_response(
        snapshot::chain_meta_with_rpc(chain, rpc),
        observed_at,
        current_set.utime_since,
        timings.validators_elected_for,
        &validator_round_data,
        history_points,
    ))
}

fn effective_validator_sets(
    current_set: ValidatorSet,
    next_set: Option<ValidatorSet>,
    observed_at: u64,
) -> (ValidatorSet, Option<ValidatorSet>) {
    if let Some(next_set) = next_set {
        if !validator_set_contains_time(&current_set, observed_at)
            && validator_set_contains_time(&next_set, observed_at)
        {
            return (next_set, None);
        }
        return (current_set, Some(next_set));
    }

    (current_set, None)
}

fn validator_set_contains_time(set: &ValidatorSet, observed_at: u64) -> bool {
    observed_at >= u64::from(set.utime_since) && observed_at < u64::from(set.utime_until)
}

fn snapshot_stale_reason(chain: &ChainConfig, snapshot: &ClockSnapshot) -> Option<String> {
    let observed_at = snapshot.fetched_at;
    let current_until = u64::from(snapshot.current_set.utime_until);
    // Every chain can serve a stale account state after an endpoint hiccup, and
    // an expired active set is the cheapest proof of it, so this check is not
    // limited to TON.
    if observed_at > current_until.saturating_add(STALE_SNAPSHOT_GRACE_SECONDS) {
        return Some(format!(
            "current validator set expired at {}",
            snapshot.current_set.utime_until
        ));
    }

    if chain.id != "ton" || snapshot.next_set.is_some() {
        return None;
    }

    let election_deadline =
        current_until.saturating_sub(u64::from(snapshot.params15.elections_end_before));
    if observed_at > election_deadline.saturating_add(STALE_SNAPSHOT_GRACE_SECONDS) {
        return Some(format!(
            "next validator set missing after election deadline {election_deadline}"
        ));
    }

    None
}

/// What a stale snapshot means to someone reading the clock, with nothing in
/// it about which endpoint served it or that a fallback was involved.
fn stale_data_warning(stale_reason: &str) -> String {
    format!("chain data is behind: {stale_reason}")
}

fn set_snapshot_warning(snapshot: &mut ClockSnapshot, warning: String) {
    if let Some(existing) = &mut snapshot.warning {
        if !existing.is_empty() {
            existing.push_str("; ");
        }
        existing.push_str(&warning);
        return;
    }

    snapshot.warning = Some(warning);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chain::test_clock_snapshot;
    use std::num::NonZeroU16;

    fn validator_set(utime_since: u32, utime_until: u32) -> ValidatorSet {
        ValidatorSet {
            utime_since,
            utime_until,
            main: NonZeroU16::new(1).unwrap(),
            total_weight: 1,
            list: Vec::new(),
        }
    }

    #[test]
    fn effective_validator_sets_promotes_active_next_set() {
        let current = validator_set(100, 200);
        let next = validator_set(200, 300);

        let (effective_current, effective_next) =
            effective_validator_sets(current, Some(next), 250);

        assert_eq!(effective_current.utime_since, 200);
        assert!(effective_next.is_none());
    }

    #[test]
    fn a_readers_warning_says_nothing_about_the_endpoints() {
        // Whatever a reader is told, it is about what they are looking at, not
        // about which RPC served it or whether a fallback was involved. One of
        // those endpoints is an address on the server itself, and none of them
        // are the reader's business.
        let warning = stale_data_warning("current validator set expired at 1788400000");

        for leak in [
            "RPC",
            "rpc",
            "fallback",
            "primary",
            "endpoint",
            "http",
            "127.0.0.1",
        ] {
            assert!(
                !warning.contains(leak),
                "a reader's warning should not mention `{leak}`: {warning}"
            );
        }
        assert!(
            warning.contains("current validator set expired"),
            "it should still say what is wrong with the data: {warning}"
        );
    }

    #[test]
    fn effective_validator_sets_keeps_active_current_set() {
        let current = validator_set(100, 200);
        let next = validator_set(200, 300);

        let (effective_current, effective_next) =
            effective_validator_sets(current, Some(next), 150);

        assert_eq!(effective_current.utime_since, 100);
        assert_eq!(effective_next.unwrap().utime_since, 200);
    }

    #[test]
    fn effective_validator_sets_keeps_future_next_set() {
        let current = validator_set(100, 200);
        let next = validator_set(250, 350);

        let (effective_current, effective_next) =
            effective_validator_sets(current, Some(next), 225);

        assert_eq!(effective_current.utime_since, 100);
        assert_eq!(effective_next.unwrap().utime_since, 250);
    }

    fn chain_config(id: &str) -> ChainConfig {
        ChainConfig {
            id: id.to_owned(),
            name: "Test".to_owned(),
            rpc: "https://example.com".to_owned(),
            rpc_fallbacks: Vec::new(),
            color: "#38bdf8".to_owned(),
            token_symbol: "TEST".to_owned(),
            rpc_label: None,
        }
    }

    #[test]
    fn ton_snapshot_without_next_set_after_election_deadline_is_stale() {
        let chain = chain_config("ton");
        let mut snapshot = test_clock_snapshot("ton");
        snapshot.current_set.utime_until = 10_000;
        snapshot.params15.elections_end_before = 1_000;
        snapshot.fetched_at = 9_000 + STALE_SNAPSHOT_GRACE_SECONDS + 1;

        let reason = snapshot_stale_reason(&chain, &snapshot).unwrap();

        assert!(reason.contains("next validator set missing"));
    }

    #[test]
    fn ton_snapshot_with_next_set_after_election_deadline_is_not_stale() {
        let chain = chain_config("ton");
        let mut snapshot = test_clock_snapshot("ton");
        snapshot.current_set.utime_until = 10_000;
        snapshot.params15.elections_end_before = 1_000;
        snapshot.fetched_at = 9_000 + STALE_SNAPSHOT_GRACE_SECONDS + 1;
        snapshot.next_set = Some(snapshot.current_set.clone());

        assert!(snapshot_stale_reason(&chain, &snapshot).is_none());
    }

    #[test]
    fn expired_active_set_is_stale_on_any_chain() {
        let chain = chain_config("everscale");
        let mut snapshot = test_clock_snapshot("everscale");
        snapshot.current_set.utime_until = 10_000;
        snapshot.fetched_at = 10_000 + STALE_SNAPSHOT_GRACE_SECONDS + 1;

        let reason = snapshot_stale_reason(&chain, &snapshot).unwrap();

        assert!(reason.contains("current validator set expired"));
    }

    #[test]
    fn non_ton_snapshot_without_next_set_after_election_deadline_is_not_stale() {
        let chain = chain_config("everscale");
        let mut snapshot = test_clock_snapshot("everscale");
        snapshot.current_set.utime_until = 10_000;
        snapshot.params15.elections_end_before = 1_000;
        snapshot.fetched_at = 9_000 + STALE_SNAPSHOT_GRACE_SECONDS + 1;

        assert!(snapshot_stale_reason(&chain, &snapshot).is_none());
    }
}
