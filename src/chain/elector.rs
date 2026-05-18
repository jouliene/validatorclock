mod election;
mod frozen;
mod snapshot;
mod toncenter;

use super::util::now_sec;
use super::{ClockSnapshot, ElectionTimingsDto, ValidatorSetDto};
use crate::config::ChainConfig;
use anyhow::{Context, Result, anyhow};
use election::fetch_election;
use frozen::fetch_frozen_validator_round_data;
use minik2::{Config, Transport, ValidatorSet};
use snapshot::previous_validator_set;
use std::env;
use tracing::debug;

pub(crate) async fn fetch_chain_snapshot(chain: &ChainConfig) -> Result<ClockSnapshot> {
    match fetch_chain_snapshot_from_jrpc(chain, &chain.rpc, None).await {
        Ok(snapshot) => Ok(snapshot),
        Err(primary_error) => {
            if chain.rpc_fallbacks.is_empty() {
                return Err(primary_error);
            }

            let primary_error = primary_error.to_string();
            let mut fallback_errors = Vec::new();
            for fallback in &chain.rpc_fallbacks {
                let warning = format!(
                    "using fallback RPC `{}`; primary RPC `{}` failed: {}",
                    super::util::endpoint_label(fallback),
                    super::util::endpoint_label(&chain.rpc),
                    primary_error
                );
                let result = if toncenter::is_toncenter_endpoint(fallback) {
                    toncenter::fetch_chain_snapshot(chain, fallback, &primary_error).await
                } else {
                    fetch_chain_snapshot_from_jrpc(chain, fallback, Some(warning)).await
                };

                match result {
                    Ok(snapshot) => return Ok(snapshot),
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
                "primary RPC failed: {}; fallback RPCs failed: {}",
                primary_error,
                fallback_errors.join("; ")
            ))
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;
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
}
