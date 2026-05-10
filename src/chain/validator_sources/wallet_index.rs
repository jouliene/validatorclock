use super::super::{ClockSnapshot, ValidatorSourceDto};
use crate::history::{RecentAbsentValidatorDto, RecentAbsentValidatorSourceDto};
use crate::validator_types::{ValidatorSourceCacheEntry, ValidatorTypeCache, contract_type_name};
use std::collections::HashSet;

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

pub(super) fn proxy_wallets_missing_source(
    cache: &ValidatorTypeCache,
    chain_id: &str,
    wallets: &[String],
) -> Vec<String> {
    wallets
        .iter()
        .filter_map(|wallet| {
            let entry = cache.get(chain_id, wallet)?;
            is_proxy_contract_type(contract_type_name(&entry.repr_hash))
                .then(|| entry.source.is_none())
                .and_then(|missing| missing.then(|| wallet.clone()))
        })
        .collect()
}

pub(super) fn single_nominator_wallets_missing_source(
    cache: &ValidatorTypeCache,
    chain_id: &str,
    wallets: &[String],
) -> Vec<String> {
    wallets
        .iter()
        .filter_map(|wallet| {
            let entry = cache.get(chain_id, wallet)?;
            is_single_nominator_contract_type(contract_type_name(&entry.repr_hash))
                .then(|| entry.source.is_none())
                .and_then(|missing| missing.then(|| wallet.clone()))
        })
        .collect()
}

pub(super) fn nominator_pool_wallets_missing_source(
    cache: &ValidatorTypeCache,
    chain_id: &str,
    wallets: &[String],
) -> Vec<String> {
    wallets
        .iter()
        .filter_map(|wallet| {
            let entry = cache.get(chain_id, wallet)?;
            is_nominator_pool_contract_type(contract_type_name(&entry.repr_hash))
                .then(|| entry.source.is_none())
                .and_then(|missing| missing.then(|| wallet.clone()))
        })
        .collect()
}

pub(super) fn validator_controller_wallets_missing_source(
    cache: &ValidatorTypeCache,
    chain_id: &str,
    wallets: &[String],
) -> Vec<String> {
    wallets
        .iter()
        .filter_map(|wallet| {
            let entry = cache.get(chain_id, wallet)?;
            is_validator_controller_contract_type(contract_type_name(&entry.repr_hash))
                .then(|| entry.source.is_none())
                .and_then(|missing| missing.then(|| wallet.clone()))
        })
        .collect()
}

pub(super) fn whales_pool_proxy_wallets_missing_source(
    cache: &ValidatorTypeCache,
    chain_id: &str,
    wallets: &[String],
) -> Vec<String> {
    wallets
        .iter()
        .filter_map(|wallet| {
            let entry = cache.get(chain_id, wallet)?;
            is_whales_pool_proxy_contract_type(contract_type_name(&entry.repr_hash))
                .then(|| entry.source.is_none())
                .and_then(|missing| missing.then(|| wallet.clone()))
        })
        .collect()
}

fn validator_source_dto(source: &ValidatorSourceCacheEntry) -> ValidatorSourceDto {
    ValidatorSourceDto {
        address: source.address.clone(),
        contract_type_hash: source.repr_hash.clone(),
    }
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
    RecentAbsentValidatorSourceDto {
        address: source.address.clone(),
        contract_type_hash: source.repr_hash.clone(),
    }
}

fn is_proxy_contract_type(contract_type: &str) -> bool {
    matches!(contract_type, "DePoolProxy" | "StEverDePoolProxy")
}

fn is_single_nominator_contract_type(contract_type: &str) -> bool {
    matches!(
        contract_type,
        "SingleNominatorV1_0" | "SingleNominatorV1_1" | "TonSingleNominatorPool"
    )
}

fn is_nominator_pool_contract_type(contract_type: &str) -> bool {
    matches!(contract_type, "TonNominatorPool")
}

fn is_validator_controller_contract_type(contract_type: &str) -> bool {
    matches!(contract_type, "ValidatorController")
}

fn is_whales_pool_proxy_contract_type(contract_type: &str) -> bool {
    matches!(contract_type, "WhalesPoolProxy")
}
