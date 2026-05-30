use super::{ChainRoundHistory, RoundHistoryStore, RoundWindow};
use crate::chain::{ClockSnapshot, ValidatorSetDto};
use std::collections::{BTreeSet, HashMap};

#[derive(Debug, Clone, Default, Eq, PartialEq)]
pub(crate) struct RoundHistoryRetention {
    pub(super) chain_rounds: HashMap<String, BTreeSet<u32>>,
}

impl RoundHistoryRetention {
    pub(super) fn from_snapshot(chain_id: &str, snapshot: &ClockSnapshot) -> Self {
        let mut retention = Self::default();
        retention.add_set(chain_id, &snapshot.current_set);
        if let Some(previous_round_id) = snapshot.current_set.round_id.checked_sub(1) {
            // Preserve the previous-color history window even when previous_set is
            // temporarily unavailable from elector/full-round data.
            retention.add_round_window(chain_id, previous_round_id);
        }
        if let Some(previous_set) = &snapshot.previous_set {
            retention.add_set(chain_id, previous_set);
        }
        if let Some(next_set) = &snapshot.next_set {
            retention.add_set(chain_id, next_set);
        }

        let election_round_id = snapshot.current_set.round_id.saturating_add(1);
        retention.add_round_window(chain_id, election_round_id);
        retention
    }

    fn add_set(&mut self, chain_id: &str, set: &ValidatorSetDto) {
        self.add_round_window(chain_id, set.round_id);
    }

    pub(super) fn add_round_window(&mut self, chain_id: &str, round_id: u32) {
        self.chain_rounds
            .entry(chain_id.to_owned())
            .or_default()
            .extend(RoundWindow::ending_at(round_id).rounds());
    }
}

impl RoundHistoryStore {
    pub(crate) fn retention_for_snapshot(
        chain_id: &str,
        snapshot: &ClockSnapshot,
    ) -> RoundHistoryRetention {
        RoundHistoryRetention::from_snapshot(chain_id, snapshot)
    }

    pub(crate) fn prune_to_retention(&mut self, retention: &RoundHistoryRetention) -> bool {
        let mut changed = false;
        for (chain_id, keep_rounds) in &retention.chain_rounds {
            if let Some(chain) = self.chains.get_mut(chain_id) {
                changed |= chain.prune_to_rounds(keep_rounds);
            }
        }
        changed |= self.remove_empty_chains();
        changed
    }
}

impl ChainRoundHistory {
    fn prune_to_rounds(&mut self, keep_rounds: &BTreeSet<u32>) -> bool {
        let before = self.rounds.len();
        self.rounds
            .retain(|round_id, round| keep_rounds.contains(round_id) && round.complete);
        self.rounds.len() != before
    }
}
