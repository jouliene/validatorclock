use super::{
    ChainRoundHistory, RoundHistoryRetention, RoundHistoryStore, StoredRound, StoredValidator,
};
use crate::chain::{ClockSnapshot, ValidatorSetDto};
use std::collections::BTreeSet;

mod round;

impl RoundHistoryStore {
    pub(crate) fn merge_from(&mut self, other: RoundHistoryStore) -> bool {
        let mut changed = false;
        for (chain_id, other_chain) in other.chains {
            changed |= self
                .chains
                .entry(chain_id)
                .or_default()
                .merge_from(other_chain);
        }
        changed
    }

    pub(crate) fn record_snapshot(
        &mut self,
        chain_id: &str,
        snapshot: &ClockSnapshot,
        observed_at: u64,
    ) -> bool {
        let retention = RoundHistoryRetention::from_snapshot(chain_id, snapshot);
        let chain = self.chains.entry(chain_id.to_owned()).or_default();
        let mut changed = chain.record_set(&snapshot.current_set, observed_at);
        if let Some(previous_set) = &snapshot.previous_set {
            changed |= chain.record_set(previous_set, observed_at);
        }
        if let Some(next_set) = &snapshot.next_set {
            changed |= chain.record_set(next_set, observed_at);
        }
        changed |= self.prune_to_retention(&retention);
        changed
    }

    pub(super) fn remove_incomplete_rounds(&mut self) -> bool {
        let mut changed = false;
        for chain in self.chains.values_mut() {
            changed |= chain.remove_incomplete_rounds();
        }
        changed |= self.remove_empty_chains();
        changed
    }

    pub(super) fn remove_empty_chains(&mut self) -> bool {
        let before = self.chains.len();
        self.chains.retain(|_, chain| !chain.rounds.is_empty());
        self.chains.len() != before
    }
}

impl ChainRoundHistory {
    pub(super) fn merge_from(&mut self, other: ChainRoundHistory) -> bool {
        let mut changed = false;
        for (round_id, other_round) in other.rounds {
            if !other_round.complete {
                continue;
            }
            match self.rounds.get_mut(&round_id) {
                Some(round) => changed |= round.merge_from(other_round),
                None => {
                    self.rounds.insert(round_id, other_round);
                    changed = true;
                }
            }
        }
        changed
    }

    pub(super) fn record_set(&mut self, set: &ValidatorSetDto, observed_at: u64) -> bool {
        if set.validators.is_empty() {
            return false;
        }

        let fake_validator_peers = fake_validator_peer_set(set);
        let incoming = StoredRound {
            round_id: set.round_id,
            round_color: set.round_color,
            utime_since: set.utime_since,
            utime_until: set.utime_until,
            observed_at,
            complete: true,
            validators: set
                .validators
                .iter()
                .map(|validator| {
                    let public_key = validator.public_key.clone();
                    let fake_node = set
                        .fake_validator_status_known
                        .then_some(fake_validator_peers.contains(&public_key.to_ascii_lowercase()));
                    let map_node = validator.map_node.clone().or_else(|| {
                        (fake_node == Some(true))
                            .then(|| {
                                self.latest_map_node_for_identity(
                                    set.round_id,
                                    &public_key,
                                    validator.wallet.as_deref(),
                                )
                            })
                            .flatten()
                    });
                    (
                        public_key,
                        StoredValidator {
                            wallet: validator.wallet.clone(),
                            map_node,
                            fake_node,
                        },
                    )
                })
                .collect(),
        };

        match self.rounds.get_mut(&set.round_id) {
            Some(existing) => existing.merge_from(incoming),
            None => {
                self.rounds.insert(set.round_id, incoming);
                true
            }
        }
    }

    fn remove_incomplete_rounds(&mut self) -> bool {
        let before = self.rounds.len();
        self.rounds.retain(|_, round| round.complete);
        self.rounds.len() != before
    }

    fn latest_map_node_for_identity(
        &self,
        round_id: u32,
        public_key: &str,
        wallet: Option<&str>,
    ) -> Option<crate::chain::ValidatorMapNodeDto> {
        self.rounds
            .range(..=round_id)
            .rev()
            .filter_map(|(_, round)| round.validator_for_identity(public_key, wallet))
            .find_map(|validator| validator.map_node.clone())
    }
}

fn fake_validator_peer_set(set: &ValidatorSetDto) -> BTreeSet<String> {
    set.fake_validator_peers
        .iter()
        .map(|peer| peer.to_ascii_lowercase())
        .filter(|peer| !peer.is_empty())
        .collect()
}
