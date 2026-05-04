use crate::chain::RoundColor;

mod participation;
mod retention;
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

fn opposite_round_color(round_color: RoundColor) -> RoundColor {
    match round_color {
        RoundColor::Blue => RoundColor::Green,
        RoundColor::Green => RoundColor::Blue,
    }
}

#[cfg(test)]
mod tests;
