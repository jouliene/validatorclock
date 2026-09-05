use super::{
    ParticipationStatus, RecentAbsentValidatorDto, RoundHistoryStore, RoundWindow,
    ValidatorParticipationDto, fake_validator_peer_set, opposite_round_color,
};
use super::{StoredRound, StoredValidator};
use crate::chain::{ClockSnapshot, RoundColor, ValidatorDto, ValidatorSetDto};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

const FAKE_VALIDATOR_MAP_GRACE_SECONDS: u64 = 60 * 60;

impl RoundHistoryStore {
    pub(crate) fn recent_mapped_validator_peers(
        &self,
        chain_id: &str,
        set: &ValidatorSetDto,
        observed_at: u64,
    ) -> HashSet<String> {
        let current_validators = ValidatorIdentitySet::from_validators(&set.validators);
        let Some(round) = self
            .chains
            .get(chain_id)
            .and_then(|chain| chain.rounds.get(&set.round_id))
        else {
            return HashSet::new();
        };

        round
            .validators
            .iter()
            .filter(|(public_key, validator)| {
                current_validators.contains(public_key, validator.wallet.as_deref())
                    && validator.map_node.is_some()
                    // A record that cannot say when the address was confirmed
                    // buys no grace. It used to fall back on when the round was
                    // last observed, which is refreshed every cycle - so a
                    // validator carrying only a remembered position was rescued
                    // from the fake mark for as long as it stayed missing,
                    // which is the opposite of what the grace is for.
                    && validator.map_seen_at.is_some_and(|seen_at| {
                        observed_at.saturating_sub(seen_at) < FAKE_VALIDATOR_MAP_GRACE_SECONDS
                    })
            })
            .map(|(public_key, _)| public_key.to_ascii_lowercase())
            .collect()
    }

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
        self.annotate_fake_validator_peers(chain_id, set);
        let fake_validator_peers = fake_validator_peer_set(set);
        // Built once for the whole set: every validator in it, and every one
        // recently absent from it, is looked for in the same five rounds.
        let window = self.same_color_window(chain_id, set.round_id, set.round_color);

        for validator in &mut set.validators {
            let is_fake = fake_validator_peers.contains(&validator.public_key.to_ascii_lowercase());
            if is_fake {
                validator.last_known_map_node = validator.map_node.clone().or_else(|| {
                    self.latest_map_node_for_identity(
                        chain_id,
                        set.round_id,
                        &validator.public_key,
                        validator.wallet.as_deref(),
                    )
                });
                validator.map_node = None;
            } else if validator.map_node.is_none() {
                // Where this validator was last seen, kept apart from where it
                // is now. History used to fill `map_node` itself, which made a
                // remembered position indistinguishable from a current one:
                // with the TON map deleted and the collector rebuilding from
                // nothing, the page still read "mapped: 393", every one of
                // them from memory. `map_node` is the map; this is the memory.
                validator.last_known_map_node = self
                    .stored_validator(chain_id, set.round_id, &validator.public_key)
                    .filter(|stored| stored.fake_node != Some(true))
                    .and_then(|stored| stored.map_node.clone());
            } else {
                validator.last_known_map_node = None;
            }
            validator.history = self.participation_in(
                chain_id,
                &window,
                &validator.public_key,
                validator.wallet.as_deref(),
            );
        }

        set.recent_absent_validators =
            self.recent_absent_validators_in(chain_id, &window, &current_validators);
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
        let window = self.same_color_window(chain_id, election_round_id, election_round_color);
        for candidate in &mut snapshot.election.candidates {
            candidate.history = self.participation_in(
                chain_id,
                &window,
                &candidate.public_key,
                Some(candidate.wallet.as_str()),
            );
        }
    }

    /// The rounds one validator set looks back over, each indexed by wallet.
    fn same_color_window(
        &self,
        chain_id: &str,
        round_id: u32,
        round_color: RoundColor,
    ) -> SameColorWindow<'_> {
        let chain = self.chains.get(chain_id);
        SameColorWindow {
            rounds: RoundWindow::ending_at(round_id)
                .rounds()
                .map(|round_id| {
                    let stored = chain
                        .and_then(|chain| chain.rounds.get(&round_id))
                        .filter(|stored| stored.round_color == round_color);
                    WindowRound {
                        round_id,
                        by_wallet: stored
                            .map(StoredRound::validators_by_wallet)
                            .unwrap_or_default(),
                        stored,
                    }
                })
                .collect(),
        }
    }

    /// One validator's history, for a test that has one round set in mind.
    /// The annotation itself builds the window once and asks it about every
    /// validator in the set.
    #[cfg(test)]
    pub(super) fn same_color_participation(
        &self,
        chain_id: &str,
        round_id: u32,
        round_color: RoundColor,
        public_key: &str,
        wallet: Option<&str>,
    ) -> Vec<ValidatorParticipationDto> {
        let window = self.same_color_window(chain_id, round_id, round_color);
        self.participation_in(chain_id, &window, public_key, wallet)
    }

    fn participation_in(
        &self,
        chain_id: &str,
        window: &SameColorWindow<'_>,
        public_key: &str,
        wallet: Option<&str>,
    ) -> Vec<ValidatorParticipationDto> {
        window
            .rounds
            .iter()
            .map(|round| {
                let (status, fake_node, map_node) = match round.validator(public_key, wallet) {
                    Some(validator) => {
                        let fake_node = validator.fake_node.unwrap_or(false);
                        let map_node = validator.map_node.clone().or_else(|| {
                            fake_node
                                .then(|| {
                                    self.latest_map_node_for_identity(
                                        chain_id,
                                        round.round_id,
                                        public_key,
                                        wallet,
                                    )
                                })
                                .flatten()
                        });
                        (ParticipationStatus::Participated, fake_node, map_node)
                    }
                    // A round nobody recorded, or one of the other colour,
                    // says nothing about this validator; a complete round that
                    // does not name it says it was not there.
                    None => match round.stored {
                        Some(stored) if stored.complete => {
                            (ParticipationStatus::Missed, false, None)
                        }
                        _ => (ParticipationStatus::Unknown, false, None),
                    },
                };
                ValidatorParticipationDto {
                    round: round.round_id,
                    status,
                    fake_node,
                    map_node,
                }
            })
            .collect()
    }

    /// As above: the annotation shares one window between the set and the
    /// validators recently absent from it.
    #[cfg(test)]
    pub(super) fn recent_absent_validators(
        &self,
        chain_id: &str,
        round_id: u32,
        round_color: RoundColor,
        current_validators: &ValidatorIdentitySet,
    ) -> Vec<RecentAbsentValidatorDto> {
        let window = self.same_color_window(chain_id, round_id, round_color);
        self.recent_absent_validators_in(chain_id, &window, current_validators)
    }

    fn recent_absent_validators_in(
        &self,
        chain_id: &str,
        window: &SameColorWindow<'_>,
        current_validators: &ValidatorIdentitySet,
    ) -> Vec<RecentAbsentValidatorDto> {
        let mut recent = BTreeMap::<String, RecentAbsentValidatorDto>::new();
        for window_round in &window.rounds {
            let Some(stored) = window_round.stored.filter(|stored| stored.complete) else {
                continue;
            };
            let round = window_round.round_id;

            for (public_key, validator) in &stored.validators {
                if current_validators.contains(public_key, validator.wallet.as_deref()) {
                    continue;
                }

                let map_node = validator.map_node.clone();
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
                        if map_node.is_some() {
                            summary.map_node = map_node.clone();
                        }
                    })
                    .or_insert_with(|| RecentAbsentValidatorDto {
                        public_key: public_key.clone(),
                        wallet: validator.wallet.clone(),
                        map_node,
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
                validator.history = self.participation_in(
                    chain_id,
                    window,
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

    fn latest_map_node_for_identity(
        &self,
        chain_id: &str,
        round_id: u32,
        public_key: &str,
        wallet: Option<&str>,
    ) -> Option<crate::chain::ValidatorMapNodeDto> {
        self.chains
            .get(chain_id)?
            .rounds
            .range(..=round_id)
            .rev()
            .filter_map(|(_, round)| round.validator_for_identity(public_key, wallet))
            .find_map(|validator| validator.map_node.clone())
    }
}

/// The same-colour rounds behind one validator set, each indexed by wallet.
///
/// A validator is looked for by public key first. A key that is not in a round
/// may still belong to the operator who was there under a different one, and
/// only the wallet says so - but finding it that way meant scanning the round's
/// whole membership, once per validator per round. TON elects a wholly new set
/// of keys every round, so the scan was the rule and not the exception, and
/// annotating one set cost the square of its size: four hundred validators
/// against five rounds of four hundred, on every request for the page.
struct SameColorWindow<'a> {
    rounds: Vec<WindowRound<'a>>,
}

struct WindowRound<'a> {
    round_id: u32,
    /// The round as recorded, when one of this colour was.
    stored: Option<&'a StoredRound>,
    by_wallet: HashMap<&'a str, &'a StoredValidator>,
}

impl<'a> WindowRound<'a> {
    fn validator(&self, public_key: &str, wallet: Option<&str>) -> Option<&'a StoredValidator> {
        let stored = self.stored?;
        stored
            .validators
            .get(public_key)
            .or_else(|| wallet.and_then(|wallet| self.by_wallet.get(wallet).copied()))
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
