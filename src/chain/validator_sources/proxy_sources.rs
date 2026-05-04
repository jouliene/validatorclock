use super::super::ValidatorSourceDto;
use super::VALIDATOR_TYPE_FETCH_CONCURRENCY;
use super::contract_types::account_contract_code_hash;
use crate::config::ChainConfig;
use anyhow::{Context, Result};
use minik2::{JrpcTransport, Transport};
use std::sync::OnceLock;
use tokio::task::JoinSet;
use tracing::{debug, warn};
use tycho_types::abi::{AbiValue, AbiVersion, FromAbi, Function, WithAbiType};
use tycho_types::boc::BocRepr;
use tycho_types::models::{MsgInfo, StdAddr, Transaction};

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

pub(super) async fn fetch_proxy_validator_sources(
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
