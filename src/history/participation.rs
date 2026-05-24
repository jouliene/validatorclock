use super::{
    ParticipationStatus, RecentAbsentValidatorDto, RoundHistoryStore, RoundWindow,
    ValidatorParticipationDto, opposite_round_color,
};
use crate::chain::{ClockSnapshot, RoundColor, ValidatorDto, ValidatorSetDto};
use std::collections::{BTreeMap, BTreeSet};

impl RoundHistoryStore {
    pub(crate) fn annotate_snapshot(&self, chain_id: &str, snapshot: &mut ClockSnapshot) {
        self.annotate_set(chain_id, &mut snapshot.current_set);
        if let Some(previous_set) = &mut snapshot.previous_set {
            self.annotate_set(chain_id, previous_set);
        }
        if let Some(next_set) = &mut snapshot.next_set {
            self.annotate_set(chain_id, next_set);
        }
        self.annotate_election_candidates(chain_id, snapshot);
    }

    fn annotate_set(&self, chain_id: &str, set: &mut ValidatorSetDto) {
        let current_validators = ValidatorIdentitySet::from_validators(&set.validators);

        for validator in &mut set.validators {
            if validator.map_node.is_none() {
                validator.map_node = self
                    .stored_validator(chain_id, set.round_id, &validator.public_key)
                    .and_then(|stored| stored.map_node.clone());
            }
            validator.history = self.same_color_participation(
                chain_id,
                set.round_id,
                set.round_color,
                &validator.public_key,
                validator.wallet.as_deref(),
            );
        }

        set.recent_absent_validators = self.recent_absent_validators(
            chain_id,
            set.round_id,
            set.round_color,
            &current_validators,
        );
        self.annotate_fake_validator_peers(chain_id, set);
    }

    fn annotate_fake_validator_peers(&self, chain_id: &str, set: &mut ValidatorSetDto) {
        if set.fake_validator_status_known {
            return;
        }

        let Some(stored) = self
            .chains
            .get(chain_id)
            .and_then(|chain| chain.rounds.get(&set.round_id))
            .filter(|stored| stored.has_fake_validator_status())
        else {
            return;
        };

        set.fake_validator_peers = stored.fake_validator_peers();
        set.fake_validator_status_known = true;
    }

    fn annotate_election_candidates(&self, chain_id: &str, snapshot: &mut ClockSnapshot) {
        if snapshot.election.candidates.is_empty() {
            return;
        }

        let election_round_id = snapshot.current_set.round_id.saturating_add(1);
        let election_round_color = opposite_round_color(snapshot.current_set.round_color);
        for candidate in &mut snapshot.election.candidates {
            candidate.history = self.same_color_participation(
                chain_id,
                election_round_id,
                election_round_color,
                &candidate.public_key,
                Some(candidate.wallet.as_str()),
            );
        }
    }

    pub(super) fn same_color_participation(
        &self,
        chain_id: &str,
        round_id: u32,
        round_color: RoundColor,
        public_key: &str,
        wallet: Option<&str>,
    ) -> Vec<ValidatorParticipationDto> {
        let chain = self.chains.get(chain_id);
        RoundWindow::ending_at(round_id)
            .rounds()
            .map(|round| {
                let (status, fake_node) = chain
                    .and_then(|chain| chain.rounds.get(&round))
                    .filter(|stored| stored.round_color == round_color)
                    .map(|stored| {
                        if stored.contains_identity(public_key, wallet) {
                            (
                                ParticipationStatus::Participated,
                                stored.fake_node_for_identity(public_key, wallet),
                            )
                        } else if stored.complete {
                            (ParticipationStatus::Missed, false)
                        } else {
                            (ParticipationStatus::Unknown, false)
                        }
                    })
                    .unwrap_or((ParticipationStatus::Unknown, false));
                ValidatorParticipationDto {
                    round,
                    status,
                    fake_node,
                }
            })
            .collect()
    }

    pub(super) fn recent_absent_validators(
        &self,
        chain_id: &str,
        round_id: u32,
        round_color: RoundColor,
        current_validators: &ValidatorIdentitySet,
    ) -> Vec<RecentAbsentValidatorDto> {
        let Some(chain) = self.chains.get(chain_id) else {
            return Vec::new();
        };

        let mut recent = BTreeMap::<String, RecentAbsentValidatorDto>::new();
        for round in RoundWindow::ending_at(round_id).rounds() {
            let Some(stored) = chain
                .rounds
                .get(&round)
                .filter(|stored| stored.round_color == round_color && stored.complete)
            else {
                continue;
            };

            for (public_key, validator) in &stored.validators {
                if current_validators.contains(public_key, validator.wallet.as_deref()) {
                    continue;
                }

                let recent_key = validator
                    .wallet
                    .clone()
                    .unwrap_or_else(|| public_key.clone());
                recent
                    .entry(recent_key)
                    .and_modify(|summary| {
                        summary.last_seen_round = round;
                        summary.public_key = public_key.clone();
                        if summary.wallet.is_none() {
                            summary.wallet = validator.wallet.clone();
                        }
                        if validator.map_node.is_some() {
                            summary.map_node = validator.map_node.clone();
                        }
                    })
                    .or_insert_with(|| RecentAbsentValidatorDto {
                        public_key: public_key.clone(),
                        wallet: validator.wallet.clone(),
                        map_node: validator.map_node.clone(),
                        source: None,
                        contract_type: None,
                        contract_type_hash: None,
                        last_seen_round: round,
                        history: Vec::new(),
                    });
            }
        }

        let mut recent: Vec<_> = recent
            .into_values()
            .map(|mut validator| {
                validator.history = self.same_color_participation(
                    chain_id,
                    round_id,
                    round_color,
                    &validator.public_key,
                    validator.wallet.as_deref(),
                );
                validator
            })
            .collect();
        recent.sort_by(|a, b| {
            b.last_seen_round
                .cmp(&a.last_seen_round)
                .then_with(|| a.public_key.cmp(&b.public_key))
        });
        recent
    }

    fn stored_validator(
        &self,
        chain_id: &str,
        round_id: u32,
        public_key: &str,
    ) -> Option<&super::StoredValidator> {
        self.chains
            .get(chain_id)
            .and_then(|chain| chain.rounds.get(&round_id))
            .and_then(|round| round.validators.get(public_key))
    }
}

pub(super) struct ValidatorIdentitySet {
    public_keys: BTreeSet<String>,
    wallets: BTreeSet<String>,
}

impl ValidatorIdentitySet {
    pub(super) fn from_validators(validators: &[ValidatorDto]) -> Self {
        Self {
            public_keys: validators
                .iter()
                .map(|validator| validator.public_key.clone())
                .collect(),
            wallets: validators
                .iter()
                .filter_map(|validator| validator.wallet.clone())
                .collect(),
        }
    }

    fn contains(&self, public_key: &str, wallet: Option<&str>) -> bool {
        self.public_keys.contains(public_key)
            || wallet.is_some_and(|wallet| self.wallets.contains(wallet))
    }
}
