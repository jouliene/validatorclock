use super::super::StoredRound;

impl StoredRound {
    pub(in crate::history) fn contains_identity(
        &self,
        public_key: &str,
        wallet: Option<&str>,
    ) -> bool {
        self.validators.contains_key(public_key)
            || wallet.is_some_and(|wallet| {
                self.validators
                    .values()
                    .any(|validator| validator.wallet.as_deref() == Some(wallet))
            })
    }

    pub(in crate::history) fn fake_node_for_identity(
        &self,
        public_key: &str,
        wallet: Option<&str>,
    ) -> bool {
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

    pub(in crate::history) fn has_fake_validator_status(&self) -> bool {
        self.validators
            .values()
            .any(|validator| validator.fake_node.is_some())
    }

    pub(in crate::history) fn fake_validator_peers(&self) -> Vec<String> {
        self.validators
            .iter()
            .filter(|(_, validator)| validator.fake_node == Some(true))
            .map(|(public_key, _)| public_key.clone())
            .collect()
    }

    pub(super) fn merge_from(&mut self, other: StoredRound) -> bool {
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
                if validator.fake_node == Some(true) {
                    validator.map_node = None;
                } else if validator.map_node.is_none() {
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
            if let Some(validator) = self.validators.get_mut(&public_key) {
                if validator.wallet.is_none() && other_validator.wallet.is_some() {
                    validator.wallet = other_validator.wallet;
                    changed = true;
                }
                if validator.fake_node.is_none() && other_validator.fake_node.is_some() {
                    validator.fake_node = other_validator.fake_node;
                    changed = true;
                }
                if validator.fake_node == Some(true) {
                    if validator.map_node.take().is_some() {
                        changed = true;
                    }
                } else if validator.map_node.is_none() && other_validator.map_node.is_some() {
                    validator.map_node = other_validator.map_node;
                    changed = true;
                }
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
