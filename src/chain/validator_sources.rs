use super::{ClockSnapshot, ValidatorSourceDto};
use crate::config::ChainConfig;
use crate::state::AppState;
use crate::validator_types::{
    ValidatorSourceCacheEntry, ValidatorTypeCache, contract_type_name, save_validator_type_cache,
};
use anyhow::{Context, Result, bail};
use minik2::{JrpcTransport, Transport};
use std::collections::HashSet;
use std::sync::OnceLock;
use tokio::task::JoinSet;
use tracing::{debug, warn};
use tycho_types::abi::{AbiValue, AbiVersion, FromAbi, Function, WithAbiType};
use tycho_types::boc::BocRepr;
use tycho_types::models::account::AccountState;
use tycho_types::models::{MsgInfo, StdAddr, Transaction};

const VALIDATOR_TYPE_FETCH_CONCURRENCY: usize = 8;
const PROXY_SOURCE_TX_SCAN_LIMIT: u8 = 100;
const PROXY_SOURCE_TX_SCAN_MAX_PAGES: usize = 40;

#[derive(Debug, Clone, FromAbi, WithAbiType)]
#[allow(dead_code)]
struct ProxyProcessNewStakeInput {
    query_id: u64,
    validator_key: minik2::HashBytes,
    stake_at: u32,
    max_factor: u32,
    adnl_addr: minik2::HashBytes,
    signature: Vec<u8>,
    elector: StdAddr,
}

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
        let cache = state.validator_type_cache.read().await;
        apply_validator_type_cache(&cache, &chain.id, snapshot);
        wallets
            .clone()
            .into_iter()
            .filter(|wallet| cache.get(&chain.id, wallet).is_none())
            .collect::<Vec<_>>()
    };

    if !missing_wallets.is_empty() {
        let fetched = fetch_validator_contract_type_hashes(chain, missing_wallets).await?;
        if !fetched.is_empty() {
            let cache_to_save = {
                let mut cache = state.validator_type_cache.write().await;
                let mut changed = false;
                for (wallet, repr_hash) in fetched {
                    changed |= cache.insert(&chain.id, &wallet, repr_hash);
                }
                apply_validator_type_cache(&cache, &chain.id, snapshot);
                changed.then(|| cache.clone())
            };

            if let Some(cache_to_save) = cache_to_save {
                save_validator_type_cache_background(state, cache_to_save).await;
            }
        }
    }

    let missing_source_wallets = {
        let cache = state.validator_type_cache.read().await;
        apply_validator_type_cache(&cache, &chain.id, snapshot);
        proxy_wallets_missing_source(&cache, &chain.id, &wallets)
    };

    if missing_source_wallets.is_empty() {
        return Ok(());
    }

    let fetched_sources = fetch_proxy_validator_sources(chain, missing_source_wallets).await?;
    if fetched_sources.is_empty() {
        return Ok(());
    }

    let cache_to_save = {
        let mut cache = state.validator_type_cache.write().await;
        let mut changed = false;
        for (wallet, source) in fetched_sources {
            changed |= cache.insert_source(
                &chain.id,
                &wallet,
                source.address,
                source.contract_type_hash,
            );
        }
        apply_validator_type_cache(&cache, &chain.id, snapshot);
        changed.then(|| cache.clone())
    };

    if let Some(cache_to_save) = cache_to_save {
        save_validator_type_cache_background(state, cache_to_save).await;
    }

    Ok(())
}

async fn save_validator_type_cache_background(state: &AppState, cache_to_save: ValidatorTypeCache) {
    let cache_path = state.validator_type_cache_path.clone();
    match tokio::task::spawn_blocking(move || {
        save_validator_type_cache(&cache_path, &cache_to_save)
    })
    .await
    {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            warn!(
                path = %state.validator_type_cache_path.display(),
                error = ?error,
                "failed to save validator type cache"
            );
        }
        Err(error) => {
            warn!(
                path = %state.validator_type_cache_path.display(),
                error = ?error,
                "validator type cache save task failed"
            );
        }
    }
}

fn validator_wallets(snapshot: &ClockSnapshot) -> Vec<String> {
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
    wallets
}

fn apply_validator_type_cache(
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
}

fn validator_source_dto(source: &ValidatorSourceCacheEntry) -> ValidatorSourceDto {
    ValidatorSourceDto {
        address: source.address.clone(),
        contract_type_hash: source.repr_hash.clone(),
    }
}

fn proxy_wallets_missing_source(
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

fn is_proxy_contract_type(contract_type: &str) -> bool {
    matches!(contract_type, "DePoolProxy" | "StEverDePoolProxy")
}

async fn fetch_validator_contract_type_hashes(
    chain: &ChainConfig,
    wallets: Vec<String>,
) -> Result<Vec<(String, String)>> {
    let transport = Transport::jrpc(&chain.rpc)
        .with_context(|| format!("invalid RPC endpoint for `{}`", chain.id))?;
    let mut fetched = Vec::new();

    for chunk in wallets.chunks(VALIDATOR_TYPE_FETCH_CONCURRENCY) {
        let mut tasks = JoinSet::new();
        for wallet in chunk {
            let transport = transport.clone();
            let wallet = wallet.clone();
            tasks.spawn(async move {
                let result = account_contract_code_hash(&transport, &wallet).await;
                (wallet, result)
            });
        }

        while let Some(result) = tasks.join_next().await {
            match result {
                Ok((wallet, Ok(repr_hash))) => fetched.push((wallet, repr_hash)),
                Ok((wallet, Err(error))) => {
                    debug!(
                        chain_id = %chain.id,
                        wallet,
                        error = ?error,
                        "failed to fetch validator contract type hash"
                    );
                }
                Err(error) => {
                    warn!(
                        chain_id = %chain.id,
                        error = ?error,
                        "validator contract type hash task failed"
                    );
                }
            }
        }
    }

    Ok(fetched)
}

async fn fetch_proxy_validator_sources(
    chain: &ChainConfig,
    wallets: Vec<String>,
) -> Result<Vec<(String, ValidatorSourceDto)>> {
    let rpc = JrpcTransport::new(&chain.rpc)
        .with_context(|| format!("invalid RPC endpoint for `{}`", chain.id))?;
    let transport = Transport::from(&rpc);
    let mut fetched = Vec::new();

    for chunk in wallets.chunks(VALIDATOR_TYPE_FETCH_CONCURRENCY) {
        let mut tasks = JoinSet::new();
        for wallet in chunk {
            let rpc = rpc.clone();
            let transport = transport.clone();
            let wallet = wallet.clone();
            tasks.spawn(async move {
                let result = discover_proxy_validator_source(&rpc, &transport, &wallet).await;
                (wallet, result)
            });
        }

        while let Some(result) = tasks.join_next().await {
            match result {
                Ok((wallet, Ok(Some(source)))) => fetched.push((wallet, source)),
                Ok((wallet, Ok(None))) => {
                    debug!(
                        chain_id = %chain.id,
                        wallet,
                        "proxy validator source not found"
                    );
                }
                Ok((wallet, Err(error))) => {
                    debug!(
                        chain_id = %chain.id,
                        wallet,
                        error = ?error,
                        "failed to discover proxy validator source"
                    );
                }
                Err(error) => {
                    warn!(
                        chain_id = %chain.id,
                        error = ?error,
                        "proxy validator source task failed"
                    );
                }
            }
        }
    }

    Ok(fetched)
}

async fn discover_proxy_validator_source(
    rpc: &JrpcTransport,
    transport: &Transport,
    proxy_wallet: &str,
) -> Result<Option<ValidatorSourceDto>> {
    let Some(address) = scan_proxy_source_address(rpc, proxy_wallet).await? else {
        return Ok(None);
    };
    let contract_type_hash = match account_contract_code_hash(transport, &address).await {
        Ok(repr_hash) => Some(repr_hash),
        Err(error) => {
            debug!(
                proxy_wallet,
                source = address,
                error = ?error,
                "failed to fetch proxy source contract hash"
            );
            None
        }
    };

    Ok(Some(ValidatorSourceDto {
        address,
        contract_type_hash,
    }))
}

async fn scan_proxy_source_address(
    rpc: &JrpcTransport,
    proxy_wallet: &str,
) -> Result<Option<String>> {
    let mut continuation = None::<String>;

    for _ in 0..PROXY_SOURCE_TX_SCAN_MAX_PAGES {
        let mut params = serde_json::json!({
            "account": proxy_wallet,
            "limit": PROXY_SOURCE_TX_SCAN_LIMIT,
        });
        if let Some(lt) = &continuation {
            params["lastTransactionLt"] = serde_json::json!(lt);
        }

        let tx_bocs: Vec<String> = rpc.call("getTransactionsList", params).await?;
        if tx_bocs.is_empty() {
            break;
        }

        let mut next_continuation = None;
        for tx_boc in tx_bocs {
            let transaction: Transaction = BocRepr::decode_base64(tx_boc)?;
            next_continuation = Some(transaction.prev_trans_lt.to_string());
            if let Some(source) = parse_proxy_process_new_stake_source(&transaction)? {
                return Ok(Some(source));
            }
        }

        if next_continuation.as_deref() == Some("0") || next_continuation == continuation {
            break;
        }
        continuation = next_continuation;
    }

    Ok(None)
}

async fn account_contract_code_hash(
    transport: &Transport,
    account_address: &str,
) -> Result<String> {
    let state = transport.get_account_state(account_address).await?;
    let account = state
        .account()
        .with_context(|| format!("account `{account_address}` not found"))?;
    let AccountState::Active(state_init) = &account.state else {
        bail!("account `{account_address}` is not active");
    };
    let code = state_init
        .code
        .as_ref()
        .with_context(|| format!("account `{account_address}` has no code"))?;
    Ok(code.repr_hash().to_string())
}

fn parse_proxy_process_new_stake_source(transaction: &Transaction) -> Result<Option<String>> {
    let Some(message) = transaction.load_in_msg()? else {
        return Ok(None);
    };

    let MsgInfo::Int(info) = message.info else {
        return Ok(None);
    };
    let Some(source) = info.src.as_std() else {
        return Ok(None);
    };
    if source.workchain != 0 {
        return Ok(None);
    }

    let values = match proxy_process_new_stake_fn().decode_internal_input(message.body) {
        Ok(values) => values,
        Err(_) => return Ok(None),
    };
    let _input = ProxyProcessNewStakeInput::from_abi(AbiValue::Tuple(values))?;

    Ok(Some(source.to_string()))
}

fn proxy_process_new_stake_fn() -> &'static Function {
    static FUNCTION: OnceLock<Function> = OnceLock::new();
    FUNCTION.get_or_init(|| {
        Function::builder(AbiVersion::V2_0, "process_new_stake")
            .with_id(0x138bac8c)
            .with_inputs(ProxyProcessNewStakeInput::abi_type().named("").flatten())
            .build()
    })
}
