mod contract_types;
mod proxy_sources;
mod wallet_index;

use self::contract_types::fetch_validator_contract_type_hashes;
use self::proxy_sources::fetch_proxy_validator_sources;
use self::wallet_index::{
    apply_validator_type_cache, proxy_wallets_missing_source, validator_wallets,
};
use super::ClockSnapshot;
use crate::config::ChainConfig;
use crate::state::AppState;
use anyhow::Result;

pub(super) const VALIDATOR_TYPE_FETCH_CONCURRENCY: usize = 8;

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
        let fetched = fetch_validator_contract_type_hashes(chain, missing_wallets).await?;
        if !fetched.is_empty() {
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
    }

    let missing_source_wallets = {
        state
            .with_validator_type_cache(|cache| {
                apply_validator_type_cache(cache, &chain.id, snapshot);
                proxy_wallets_missing_source(cache, &chain.id, &wallets)
            })
            .await
    };

    if missing_source_wallets.is_empty() {
        return Ok(());
    }

    let fetched_sources = fetch_proxy_validator_sources(chain, missing_source_wallets).await?;
    if fetched_sources.is_empty() {
        return Ok(());
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

    Ok(())
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
