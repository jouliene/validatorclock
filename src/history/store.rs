use super::{
    ChainRoundHistory, RoundHistoryRetention, RoundHistoryStore, StoredRound, StoredValidator,
};
use crate::chain::{ClockSnapshot, ValidatorSetDto};
use std::collections::BTreeSet;

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
                    (
                        validator.public_key.clone(),
                        StoredValidator {
                            wallet: validator.wallet.clone(),
                            map_node: validator.map_node.clone(),
                            fake_node: set.fake_validator_status_known.then_some(
                                fake_validator_peers
                                    .contains(&validator.public_key.to_ascii_lowercase()),
                            ),
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
}

impl StoredRound {
    pub(super) fn contains_identity(&self, public_key: &str, wallet: Option<&str>) -> bool {
        self.validators.contains_key(public_key)
            || wallet.is_some_and(|wallet| {
                self.validators
                    .values()
                    .any(|validator| validator.wallet.as_deref() == Some(wallet))
            })
    }

    pub(super) fn fake_node_for_identity(&self, public_key: &str, wallet: Option<&str>) -> bool {
        self.validators
            .get(public_key)
            .and_then(|validator| validator.fake_node)
            .or_else(|| {
                wallet.and_then(|wallet| {
                    self.validators
                        .values()
                        .find(|validator| validator.wallet.as_deref() == Some(wallet))
                        .and_then(|validator| validator.fake_node)
                })
            })
            .unwrap_or(false)
    }

    pub(super) fn has_fake_validator_status(&self) -> bool {
        self.validators
            .values()
            .any(|validator| validator.fake_node.is_some())
    }

    pub(super) fn fake_validator_peers(&self) -> Vec<String> {
        self.validators
            .iter()
            .filter(|(_, validator)| validator.fake_node == Some(true))
            .map(|(public_key, _)| public_key.clone())
            .collect()
    }

    fn merge_from(&mut self, other: StoredRound) -> bool {
        if !other.complete {
            return false;
        }
        if !self.complete {
            return self.replace_with_preserved_wallets(other);
        }
        self.merge_complete_from(other)
    }

    fn merge_complete_from(&mut self, other: StoredRound) -> bool {
        let other_is_preferred = other.observed_at > self.observed_at
            || (other.observed_at == self.observed_at && other.richness() > self.richness());
        if other_is_preferred {
            self.replace_with_preserved_wallets(other)
        } else {
            self.merge_missing_validator_data(other)
        }
    }

    fn replace_with_preserved_wallets(&mut self, mut replacement: StoredRound) -> bool {
        for (public_key, validator) in &mut replacement.validators {
            if let Some(existing) = self.validators.get(public_key) {
                if validator.wallet.is_none()
                    && let Some(wallet) = existing.wallet.clone()
                {
                    validator.wallet = Some(wallet);
                }
                if validator.fake_node.is_none() {
                    validator.fake_node = existing.fake_node;
                }
                if validator.map_node.is_none() {
                    validator.map_node = existing.map_node.clone();
                }
            }
        }

        if self.same_meaningful_content(&replacement) {
            return false;
        }

        *self = replacement;
        true
    }

    fn merge_missing_validator_data(&mut self, other: StoredRound) -> bool {
        let mut changed = false;
        let observed_at = other.observed_at;
        for (public_key, other_validator) in other.validators {
            if let Some(validator) = self.validators.get_mut(&public_key)
                && validator.wallet.is_none()
                && other_validator.wallet.is_some()
            {
                validator.wallet = other_validator.wallet;
                changed = true;
            }
            if let Some(validator) = self.validators.get_mut(&public_key)
                && validator.fake_node.is_none()
                && other_validator.fake_node.is_some()
            {
                validator.fake_node = other_validator.fake_node;
                changed = true;
            }
            if let Some(validator) = self.validators.get_mut(&public_key)
                && validator.map_node.is_none()
                && other_validator.map_node.is_some()
            {
                validator.map_node = other_validator.map_node;
                changed = true;
            }
        }
        if changed {
            self.observed_at = self.observed_at.max(observed_at);
        }
        changed
    }

    fn same_meaningful_content(&self, other: &StoredRound) -> bool {
        self.round_id == other.round_id
            && self.round_color == other.round_color
            && self.utime_since == other.utime_since
            && self.utime_until == other.utime_until
            && self.complete == other.complete
            && self.validators == other.validators
    }

    fn richness(&self) -> (usize, usize, usize, usize) {
        (
            self.validators.len(),
            self.validators
                .values()
                .filter(|validator| validator.wallet.is_some())
                .count(),
            self.validators
                .values()
                .filter(|validator| validator.fake_node.is_some())
                .count(),
            self.validators
                .values()
                .filter(|validator| validator.map_node.is_some())
                .count(),
        )
    }
}

fn fake_validator_peer_set(set: &ValidatorSetDto) -> BTreeSet<String> {
    set.fake_validator_peers
        .iter()
        .map(|peer| peer.to_ascii_lowercase())
        .filter(|peer| !peer.is_empty())
        .collect()
}
