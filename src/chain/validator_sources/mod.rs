mod contract_types;
mod hipo_validator_proxy_sources;
mod nominator_pool_sources;
mod proxy_sources;
mod single_nominator_sources;
mod validator_controller_sources;
mod wallet_index;
mod whales_pool_proxy_sources;

use self::contract_types::fetch_validator_contract_type_hashes;
use self::hipo_validator_proxy_sources::fetch_hipo_validator_proxy_sources;
use self::nominator_pool_sources::fetch_nominator_pool_validator_sources;
use self::proxy_sources::fetch_proxy_validator_sources;
use self::single_nominator_sources::fetch_single_nominator_validator_sources;
use self::validator_controller_sources::fetch_validator_controller_sources;
use self::wallet_index::{ValidatorSourceKind, apply_validator_type_cache, validator_wallets};
use self::whales_pool_proxy_sources::fetch_whales_pool_proxy_sources;
use super::{ClockSnapshot, ValidatorSourceDto};
use crate::config::ChainConfig;
use crate::state::AppState;
use anyhow::Result;

pub(super) const VALIDATOR_TYPE_FETCH_CONCURRENCY: usize = 8;
const SOURCE_RESOLVERS: &[ValidatorSourceKind] = &[
    ValidatorSourceKind::Proxy,
    ValidatorSourceKind::SingleNominator,
    ValidatorSourceKind::NominatorPool,
    ValidatorSourceKind::ValidatorController,
    ValidatorSourceKind::WhalesPoolProxy,
    ValidatorSourceKind::HipoValidatorProxy,
];

pub(super) async fn update_validator_contract_type_hashes(
    state: &AppState,
    chain: &ChainConfig,
    snapshot: &mut ClockSnapshot,
) -> Result<()> {
    let wallets = validator_wallets(snapshot);
    if wallets.is_empty() {
        return Ok(());
    }

    let missing_wallets = {
        state
            .with_validator_type_cache(|cache| {
                apply_validator_type_cache(cache, &chain.id, snapshot);
                wallets
                    .clone()
                    .into_iter()
                    .filter(|wallet| cache.get(&chain.id, wallet).is_none())
                    .collect::<Vec<_>>()
            })
            .await
    };

    if !missing_wallets.is_empty() {
        for chunk in missing_wallets.chunks(VALIDATOR_TYPE_FETCH_CONCURRENCY) {
            let fetched = fetch_validator_contract_type_hashes(chain, chunk.to_vec()).await?;
            cache_validator_contract_type_hashes(state, chain, snapshot, fetched).await;
        }
    }

    for resolver in SOURCE_RESOLVERS {
        let missing_source_wallets = {
            state
                .with_validator_type_cache(|cache| {
                    apply_validator_type_cache(cache, &chain.id, snapshot);
                    resolver.wallets_missing_source(cache, &chain.id, &wallets)
                })
                .await
        };

        if !missing_source_wallets.is_empty() {
            let fetched_sources =
                fetch_validator_sources(*resolver, chain, missing_source_wallets).await?;
            cache_validator_sources(state, chain, snapshot, fetched_sources).await;
        }
    }

    Ok(())
}

async fn fetch_validator_sources(
    resolver: ValidatorSourceKind,
    chain: &ChainConfig,
    wallets: Vec<String>,
) -> Result<Vec<(String, ValidatorSourceDto)>> {
    match resolver {
        ValidatorSourceKind::Proxy => fetch_proxy_validator_sources(chain, wallets).await,
        ValidatorSourceKind::SingleNominator => {
            fetch_single_nominator_validator_sources(chain, wallets).await
        }
        ValidatorSourceKind::NominatorPool => {
            fetch_nominator_pool_validator_sources(chain, wallets).await
        }
        ValidatorSourceKind::ValidatorController => {
            fetch_validator_controller_sources(chain, wallets).await
        }
        ValidatorSourceKind::WhalesPoolProxy => {
            fetch_whales_pool_proxy_sources(chain, wallets).await
        }
        ValidatorSourceKind::HipoValidatorProxy => {
            fetch_hipo_validator_proxy_sources(chain, wallets).await
        }
    }
}

async fn cache_validator_sources(
    state: &AppState,
    chain: &ChainConfig,
    snapshot: &mut ClockSnapshot,
    fetched_sources: Vec<(String, super::ValidatorSourceDto)>,
) {
    if fetched_sources.is_empty() {
        return;
    }

    let cache_to_save = state
        .update_validator_type_cache(|cache| {
            let mut changed = false;
            for (wallet, source) in fetched_sources {
                changed |= cache.insert_source(
                    &chain.id,
                    &wallet,
                    source.address,
                    source.contract_type_hash,
                );
            }
            apply_validator_type_cache(cache, &chain.id, snapshot);
            changed.then(|| cache.clone())
        })
        .await;

    if let Some(cache_to_save) = cache_to_save {
        state
            .save_validator_type_cache_background(cache_to_save)
            .await;
    }
}

async fn cache_validator_contract_type_hashes(
    state: &AppState,
    chain: &ChainConfig,
    snapshot: &mut ClockSnapshot,
    fetched: Vec<(String, String)>,
) {
    if fetched.is_empty() {
        return;
    }

    let cache_to_save = state
        .update_validator_type_cache(|cache| {
            let mut changed = false;
            for (wallet, repr_hash) in fetched {
                changed |= cache.insert(&chain.id, &wallet, repr_hash);
            }
            apply_validator_type_cache(cache, &chain.id, snapshot);
            changed.then(|| cache.clone())
        })
        .await;

    if let Some(cache_to_save) = cache_to_save {
        state
            .save_validator_type_cache_background(cache_to_save)
            .await;
    }
}

pub(super) async fn apply_cached_validator_contract_type_hashes(
    state: &AppState,
    chain: &ChainConfig,
    snapshot: &mut ClockSnapshot,
) {
    state
        .with_validator_type_cache(|cache| {
            apply_validator_type_cache(cache, &chain.id, snapshot);
        })
        .await;
}
