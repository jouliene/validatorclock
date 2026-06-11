mod election;
mod frozen;
mod snapshot;
mod toncenter;
mod toncenter_stack;

use super::dto::{RoundStatsColorDto, RoundStatsPointDto, ValidatorRoundData};
use super::util::{endpoint_label, now_sec, round_color};
use super::{
    ChainMeta, ChainRoundStatsDto, ClockSnapshot, ElectionTimingsDto, RoundColor, ValidatorSetDto,
};
use crate::config::ChainConfig;
use anyhow::{Context, Result, anyhow};
use election::fetch_election;
use frozen::fetch_frozen_validator_round_data;
use minik2::{Config, Transport, ValidatorSet};
use snapshot::previous_validator_set;
use std::collections::BTreeMap;
use std::env;
use tracing::debug;

const TON_STALE_GRACE_SECONDS: u64 = 300;
const ROUND_STATS_LIMIT_PER_COLOR: usize = 5;
const SECONDS_PER_YEAR: f64 = 365.0 * 24.0 * 60.0 * 60.0;

pub(crate) async fn fetch_chain_snapshot(chain: &ChainConfig) -> Result<ClockSnapshot> {
    match fetch_chain_snapshot_from_endpoint(chain, &chain.rpc, None).await {
        Ok(mut snapshot) => {
            if let Some(stale_reason) = snapshot_stale_reason(chain, &snapshot) {
                let primary_reason = format!("appears stale: {stale_reason}");
                match fetch_fallback_snapshot(chain, &primary_reason, true).await {
                    Ok(snapshot) => return Ok(snapshot),
                    Err(error) => set_snapshot_warning(
                        &mut snapshot,
                        format!(
                            "primary RPC `{}` appears stale: {}; {}",
                            super::util::endpoint_label(&chain.rpc),
                            stale_reason,
                            error
                        ),
                    ),
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

pub(crate) fn chain_round_stats_from_history(
    snapshot: &ClockSnapshot,
    history_points: Vec<RoundStatsPointDto>,
) -> ChainRoundStatsDto {
    build_round_stats_response(
        snapshot.chain.clone(),
        snapshot.fetched_at,
        snapshot.current_set.utime_since,
        snapshot.params15.validators_elected_for,
        &std::collections::HashMap::new(),
        &history_points,
    )
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
        let warning = format!(
            "using fallback RPC `{}`; primary RPC `{}` {}",
            super::util::endpoint_label(fallback),
            super::util::endpoint_label(&chain.rpc),
            primary_reason
        );

        match fetch_chain_snapshot_from_endpoint(chain, fallback, Some(warning)).await {
            Ok(mut snapshot) => {
                if let Some(stale_reason) = snapshot_stale_reason(chain, &snapshot) {
                    if require_fresh {
                        fallback_errors.push(format!(
                            "{} returned stale snapshot: {}",
                            super::util::endpoint_label(fallback),
                            stale_reason
                        ));
                        continue;
                    }

                    set_snapshot_warning(
                        &mut snapshot,
                        format!(
                            "fallback RPC `{}` returned stale snapshot: {}",
                            super::util::endpoint_label(fallback),
                            stale_reason
                        ),
                    );
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
    warning: Option<String>,
) -> Result<ClockSnapshot> {
    if toncenter::is_toncenter_endpoint(rpc) {
        return toncenter::fetch_chain_snapshot(chain, rpc, warning).await;
    }

    fetch_chain_snapshot_from_jrpc(chain, rpc, warning).await
}

async fn fetch_chain_round_stats_from_endpoint(
    chain: &ChainConfig,
    rpc: &str,
    history_points: &[RoundStatsPointDto],
) -> Result<ChainRoundStatsDto> {
    if toncenter::is_toncenter_endpoint(rpc) {
        return toncenter::fetch_chain_round_stats(chain, rpc, history_points).await;
    }

    fetch_chain_round_stats_from_jrpc(chain, rpc, history_points).await
}

async fn fetch_chain_snapshot_from_jrpc(
    chain: &ChainConfig,
    rpc: &str,
    warning: Option<String>,
) -> Result<ClockSnapshot> {
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
            if env::var_os("VALIDATORS_CLOCK_DEBUG_HISTORY").is_some() {
                debug!(error = ?error, "validator round data failed");
            }
            Default::default()
        }
    };

    Ok(ClockSnapshot {
        chain: snapshot::chain_meta_with_rpc(chain, rpc),
        selected_endpoint: Some(rpc.to_owned()),
        fetched_at: observed_at,
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
        warning,
    })
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

pub(super) fn build_round_stats_response(
    chain: ChainMeta,
    fetched_at: u64,
    active_utime_since: u32,
    validators_elected_for: u32,
    validator_round_data: &std::collections::HashMap<u32, ValidatorRoundData>,
    history_points: &[RoundStatsPointDto],
) -> ChainRoundStatsDto {
    let active_round_id = active_utime_since / validators_elected_for.max(1);
    let mut points_by_round = BTreeMap::new();
    for point in history_points
        .iter()
        .filter(|point| point.utime_since < active_utime_since)
    {
        points_by_round.insert(point.round_id, point.clone());
    }
    for point in validator_round_data
        .iter()
        .filter(|(utime_since, _)| **utime_since < active_utime_since)
        .filter_map(|(utime_since, round_data)| {
            round_stats_point(*utime_since, validators_elected_for, round_data)
        })
    {
        points_by_round.insert(point.round_id, point);
    }
    let points: Vec<_> = points_by_round.into_values().collect();

    ChainRoundStatsDto {
        chain,
        fetched_at,
        active_round_id,
        active_round_color: round_color(active_round_id),
        blue: round_stats_color(RoundColor::Blue, &points),
        green: round_stats_color(RoundColor::Green, &points),
    }
}

fn round_stats_color(color: RoundColor, points: &[RoundStatsPointDto]) -> RoundStatsColorDto {
    let mut rounds: Vec<_> = points
        .iter()
        .filter(|point| point.round_color == color)
        .rev()
        .take(ROUND_STATS_LIMIT_PER_COLOR)
        .cloned()
        .collect();
    rounds.reverse();

    RoundStatsColorDto {
        round_color: color,
        rounds,
    }
}

fn round_stats_point(
    utime_since: u32,
    validators_elected_for: u32,
    round_data: &ValidatorRoundData,
) -> Option<RoundStatsPointDto> {
    if round_data.validators.is_empty() {
        return None;
    }

    let round_id = utime_since / validators_elected_for.max(1);
    let (min_stake, max_stake) = min_max_stakes(round_data);
    let profitability_percent =
        profitability_percent(utime_since, validators_elected_for, round_data);

    Some(RoundStatsPointDto {
        round_id,
        round_color: round_color(round_id),
        utime_since,
        utime_until: utime_since.saturating_add(validators_elected_for),
        validator_count: round_data.validators.len(),
        total_stake: round_data.total_stake.clone(),
        total_stake_raw: round_data.total_stake_raw.clone(),
        min_stake,
        max_stake,
        total_reward: round_data.total_reward.clone(),
        total_reward_raw: round_data.total_reward_raw.clone(),
        profitability_percent,
    })
}

fn min_max_stakes(round_data: &ValidatorRoundData) -> (Option<String>, Option<String>) {
    let mut stakes = round_data.validators.values().filter_map(|validator| {
        parse_decimal(&validator.stake).map(|value| (value, &validator.stake))
    });

    let Some(first) = stakes.next() else {
        return (None, None);
    };

    let (min, max) = stakes.fold((first, first), |(min, max), stake| {
        let min = if stake.0.total_cmp(&min.0).is_lt() {
            stake
        } else {
            min
        };
        let max = if stake.0.total_cmp(&max.0).is_gt() {
            stake
        } else {
            max
        };
        (min, max)
    });

    (Some(min.1.clone()), Some(max.1.clone()))
}

fn profitability_percent(
    utime_since: u32,
    validators_elected_for: u32,
    round_data: &ValidatorRoundData,
) -> Option<f64> {
    let duration = validators_elected_for.max(1) as f64;
    let stake = round_data
        .total_stake_raw
        .as_deref()
        .and_then(parse_decimal)
        .or_else(|| round_data.total_stake.as_deref().and_then(parse_decimal))?;
    let reward = round_data
        .total_reward_raw
        .as_deref()
        .and_then(parse_decimal)
        .or_else(|| round_data.total_reward.as_deref().and_then(parse_decimal))?;

    if stake <= 0.0 || reward < 0.0 || utime_since == 0 {
        return None;
    }

    Some(reward / stake * (SECONDS_PER_YEAR / (duration * 2.0)) * 100.0)
}

fn parse_decimal(value: &str) -> Option<f64> {
    let parsed = value.replace(',', "").parse::<f64>().ok()?;
    parsed.is_finite().then_some(parsed)
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
    if chain.id != "ton" {
        return None;
    }

    let observed_at = snapshot.fetched_at;
    let current_until = u64::from(snapshot.current_set.utime_until);
    if observed_at > current_until.saturating_add(TON_STALE_GRACE_SECONDS) {
        return Some(format!(
            "current validator set expired at {}",
            snapshot.current_set.utime_until
        ));
    }

    if snapshot.next_set.is_some() {
        return None;
    }

    let election_deadline =
        current_until.saturating_sub(u64::from(snapshot.params15.elections_end_before));
    if observed_at > election_deadline.saturating_add(TON_STALE_GRACE_SECONDS) {
        return Some(format!(
            "next validator set missing after election deadline {election_deadline}"
        ));
    }

    None
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
        snapshot.fetched_at = 9_000 + TON_STALE_GRACE_SECONDS + 1;

        let reason = snapshot_stale_reason(&chain, &snapshot).unwrap();

        assert!(reason.contains("next validator set missing"));
    }

    #[test]
    fn ton_snapshot_with_next_set_after_election_deadline_is_not_stale() {
        let chain = chain_config("ton");
        let mut snapshot = test_clock_snapshot("ton");
        snapshot.current_set.utime_until = 10_000;
        snapshot.params15.elections_end_before = 1_000;
        snapshot.fetched_at = 9_000 + TON_STALE_GRACE_SECONDS + 1;
        snapshot.next_set = Some(snapshot.current_set.clone());

        assert!(snapshot_stale_reason(&chain, &snapshot).is_none());
    }

    #[test]
    fn non_ton_snapshot_without_next_set_after_election_deadline_is_not_stale() {
        let chain = chain_config("everscale");
        let mut snapshot = test_clock_snapshot("everscale");
        snapshot.current_set.utime_until = 10_000;
        snapshot.params15.elections_end_before = 1_000;
        snapshot.fetched_at = 9_000 + TON_STALE_GRACE_SECONDS + 1;

        assert!(snapshot_stale_reason(&chain, &snapshot).is_none());
    }
}
