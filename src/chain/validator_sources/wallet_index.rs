use super::super::{ClockSnapshot, ValidatorSourceDto};
use crate::history::{RecentAbsentValidatorDto, RecentAbsentValidatorSourceDto};
use crate::validator_types::{ValidatorSourceCacheEntry, ValidatorTypeCache, contract_type_name};
use std::collections::HashSet;

#[derive(Clone, Copy)]
pub(super) enum ValidatorSourceKind {
    Proxy,
    SingleNominator,
    NominatorPool,
    ValidatorController,
    WhalesPoolProxy,
    HipoValidatorProxy,
    StEverStrategy,
}

impl ValidatorSourceKind {
    pub(super) fn wallets_missing_source(
        self,
        cache: &ValidatorTypeCache,
        chain_id: &str,
        wallets: &[String],
    ) -> Vec<String> {
        wallets
            .iter()
            .filter_map(|wallet| {
                let entry = cache.get(chain_id, wallet)?;
                self.matches_contract_type(contract_type_name(&entry.repr_hash))
                    .then(|| self.source_needs_refresh(entry.source.as_ref()))
                    .and_then(|missing| missing.then(|| wallet.clone()))
            })
            .collect()
    }

    fn source_needs_refresh(self, source: Option<&ValidatorSourceCacheEntry>) -> bool {
        match source {
            None => true,
            Some(source) => self.requires_source_contract_hash() && source.repr_hash.is_none(),
        }
    }

    fn requires_source_contract_hash(self) -> bool {
        matches!(self, Self::Proxy | Self::StEverStrategy)
    }

    fn matches_contract_type(self, contract_type: &str) -> bool {
        match self {
            Self::Proxy => matches!(contract_type, "DePoolProxy" | "StEverDePoolProxy"),
            Self::SingleNominator => matches!(
                contract_type,
                "SingleNominatorV1_0" | "SingleNominatorV1_1" | "TonSingleNominatorPool"
            ),
            Self::NominatorPool => matches!(contract_type, "TonNominatorPool"),
            Self::ValidatorController => matches!(contract_type, "ValidatorController"),
            Self::WhalesPoolProxy => matches!(contract_type, "WhalesPoolProxy"),
            Self::HipoValidatorProxy => matches!(contract_type, "HipoValidatorProxy"),
            Self::StEverStrategy => matches!(contract_type, "StEverStrategy"),
        }
    }
}

pub(super) fn validator_wallets(snapshot: &ClockSnapshot) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut wallets = Vec::new();
    for validator in snapshot
        .current_set
        .validators
        .iter()
        .chain(
            snapshot
                .previous_set
                .iter()
                .flat_map(|set| set.validators.iter()),
        )
        .chain(
            snapshot
                .next_set
                .iter()
                .flat_map(|set| set.validators.iter()),
        )
    {
        if let Some(wallet) = &validator.wallet
            && seen.insert(wallet.clone())
        {
            wallets.push(wallet.clone());
        }
    }
    for candidate in &snapshot.election.candidates {
        if seen.insert(candidate.wallet.clone()) {
            wallets.push(candidate.wallet.clone());
        }
    }
    for validator in snapshot
        .current_set
        .recent_absent_validators
        .iter()
        .chain(
            snapshot
                .previous_set
                .iter()
                .flat_map(|set| set.recent_absent_validators.iter()),
        )
        .chain(
            snapshot
                .next_set
                .iter()
                .flat_map(|set| set.recent_absent_validators.iter()),
        )
    {
        if let Some(wallet) = &validator.wallet
            && seen.insert(wallet.clone())
        {
            wallets.push(wallet.clone());
        }
    }
    wallets
}

pub(super) fn apply_validator_type_cache(
    cache: &ValidatorTypeCache,
    chain_id: &str,
    snapshot: &mut ClockSnapshot,
) {
    for validator in snapshot
        .current_set
        .validators
        .iter_mut()
        .chain(
            snapshot
                .previous_set
                .iter_mut()
                .flat_map(|set| set.validators.iter_mut()),
        )
        .chain(
            snapshot
                .next_set
                .iter_mut()
                .flat_map(|set| set.validators.iter_mut()),
        )
    {
        if let Some(wallet) = &validator.wallet
            && let Some(entry) = cache.get(chain_id, wallet)
        {
            validator.contract_type_hash = Some(entry.repr_hash.clone());
            validator.contract_type = Some(contract_type_name(&entry.repr_hash).to_owned());
            validator.source = entry.source.as_ref().map(validator_source_dto);
        }
    }

    for candidate in &mut snapshot.election.candidates {
        if let Some(entry) = cache.get(chain_id, &candidate.wallet) {
            candidate.contract_type_hash = Some(entry.repr_hash.clone());
            candidate.contract_type = Some(contract_type_name(&entry.repr_hash).to_owned());
            candidate.source = entry.source.as_ref().map(validator_source_dto);
        }
    }

    for validator in snapshot
        .current_set
        .recent_absent_validators
        .iter_mut()
        .chain(
            snapshot
                .previous_set
                .iter_mut()
                .flat_map(|set| set.recent_absent_validators.iter_mut()),
        )
        .chain(
            snapshot
                .next_set
                .iter_mut()
                .flat_map(|set| set.recent_absent_validators.iter_mut()),
        )
    {
        apply_recent_absent_validator_type_cache(cache, chain_id, validator);
    }
}

fn validator_source_dto(source: &ValidatorSourceCacheEntry) -> ValidatorSourceDto {
    ValidatorSourceDto::new(source.address.clone(), source.repr_hash.clone())
}

fn apply_recent_absent_validator_type_cache(
    cache: &ValidatorTypeCache,
    chain_id: &str,
    validator: &mut RecentAbsentValidatorDto,
) {
    let Some(wallet) = &validator.wallet else {
        return;
    };
    let Some(entry) = cache.get(chain_id, wallet) else {
        return;
    };

    validator.contract_type_hash = Some(entry.repr_hash.clone());
    validator.contract_type = Some(contract_type_name(&entry.repr_hash).to_owned());
    validator.source = entry.source.as_ref().map(recent_absent_source_dto);
}

fn recent_absent_source_dto(source: &ValidatorSourceCacheEntry) -> RecentAbsentValidatorSourceDto {
    RecentAbsentValidatorSourceDto::new(source.address.clone(), source.repr_hash.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validator_types::ValidatorTypeCache;

    const ST_EVER_STRATEGY_HASH: &str =
        "0ac45261b93f5679c35bc4d2d059a759b24709492bb5e3d143d92931356fc0bb";
    const VALIDATOR_CONTROLLER_HASH: &str =
        "dd3ce98db487c7585803933bffba7a57eb4e663099059d08b83db0b4ce060793";

    #[test]
    fn st_ever_strategy_source_without_hash_is_refreshed() {
        let chain_id = "tycho-testnet";
        let wallet = "-1:95cac926ba69119a8d6632e3fe903c8638aca419242a07d99c2393b02e333a8c";
        let mut cache = ValidatorTypeCache::default();
        cache.insert(chain_id, wallet, ST_EVER_STRATEGY_HASH.to_owned());
        cache.insert_source(
            chain_id,
            wallet,
            "0:0eae99fabded69b43c026eee55748eb2c4baa22c6d9608350c797fcaed227409".to_owned(),
            None,
        );

        assert_eq!(
            ValidatorSourceKind::StEverStrategy.wallets_missing_source(
                &cache,
                chain_id,
                &[wallet.to_owned()]
            ),
            vec![wallet.to_owned()]
        );
    }

    #[test]
    fn validator_controller_source_without_hash_is_kept() {
        let chain_id = "ton";
        let wallet = "-1:e087b639336eeb3a546acafa4ab1b5bc2f4cfb7ecd889eeb717ce9ec7d208033";
        let mut cache = ValidatorTypeCache::default();
        cache.insert(chain_id, wallet, VALIDATOR_CONTROLLER_HASH.to_owned());
        cache.insert_source(
            chain_id,
            wallet,
            "0:a45b17f28409229b78360e3290420f13e4fe20f90d7e2bf8c4ac6703259e22fa".to_owned(),
            None,
        );

        assert!(
            ValidatorSourceKind::ValidatorController
                .wallets_missing_source(&cache, chain_id, &[wallet.to_owned()])
                .is_empty()
        );
    }
}
