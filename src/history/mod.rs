use crate::chain::RoundColor;

const HISTORY_DEPTH: usize = 5;

mod participation;
mod retention;
mod storage;
mod store;
mod types;

pub(crate) use retention::RoundHistoryRetention;
pub(crate) use storage::{
    load_round_history_for_chains, round_history_chain_path, save_round_history_merged,
};
use types::{ChainRoundHistory, RoundHistoryDisk, StoredRound, StoredValidator};
pub(crate) use types::{
    ParticipationStatus, RecentAbsentValidatorDto, RoundHistoryStore, ValidatorParticipationDto,
};

fn same_color_rounds(round_id: u32) -> Vec<u32> {
    (0..HISTORY_DEPTH)
        .rev()
        .filter_map(|index| round_id.checked_sub((index * 2) as u32))
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
