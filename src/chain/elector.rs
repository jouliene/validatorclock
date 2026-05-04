mod election;
mod frozen;
mod snapshot;

use super::util::now_sec;
use super::{ChainMeta, ClockSnapshot, ElectionTimingsDto, ValidatorSetDto};
use crate::config::ChainConfig;
use anyhow::{Context, Result};
use election::fetch_election;
use frozen::fetch_frozen_validator_round_data;
use minik2::{Config, Transport};
use snapshot::previous_validator_set;
use std::env;
use tracing::debug;

pub(crate) async fn fetch_chain_snapshot(chain: &ChainConfig) -> Result<ClockSnapshot> {
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
