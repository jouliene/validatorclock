use crate::chain::{RoundColor, ValidatorSetDto};
use std::collections::BTreeSet;

mod participation;
mod retention;
mod stats;
mod storage;
mod store;
mod types;
mod window;

pub(crate) use retention::RoundHistoryRetention;
pub(crate) use storage::{
    load_round_history_for_chains, round_history_chain_path, save_round_history_merged,
};
use types::{ChainRoundHistory, RoundHistoryDisk, StoredRound, StoredValidator};
pub(crate) use types::{
    ParticipationStatus, RecentAbsentValidatorDto, RecentAbsentValidatorSourceDto,
    RoundHistoryStore, ValidatorParticipationDto,
};
use window::RoundWindow;

/// The peers a set calls fake, in the form the store compares them in: folded
/// to lower case, with the empty ones dropped.
fn fake_validator_peer_set(set: &ValidatorSetDto) -> BTreeSet<String> {
    set.fake_validator_peers
        .iter()
        .map(|peer| peer.to_ascii_lowercase())
        .filter(|peer| !peer.is_empty())
        .collect()
}

fn opposite_round_color(round_color: RoundColor) -> RoundColor {
    match round_color {
        RoundColor::Blue => RoundColor::Green,
        RoundColor::Green => RoundColor::Blue,
    }
}

#[cfg(test)]
mod tests;
