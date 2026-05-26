use super::super::ValidatorSourceDto;
use super::contract_types::account_contract_code_hash;
use super::provider::ValidatorSourceProvider;
use super::wallet_tasks::fetch_wallet_tasks;
use anyhow::Result;
use std::sync::OnceLock;
use tracing::debug;
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
    chain_id: &str,
    provider: &ValidatorSourceProvider,
    wallets: Vec<String>,
) -> Result<Vec<(String, ValidatorSourceDto)>> {
    Ok(fetch_wallet_tasks(
        chain_id,
        provider,
        wallets,
        Some("proxy validator source not found"),
        "failed to discover proxy validator source",
        "proxy validator source task failed",
        |provider, wallet| async move { discover_proxy_validator_source(&provider, &wallet).await },
    )
    .await)
}

async fn discover_proxy_validator_source(
    provider: &ValidatorSourceProvider,
    proxy_wallet: &str,
) -> Result<Option<ValidatorSourceDto>> {
    let Some(address) = scan_proxy_source_address(provider, proxy_wallet).await? else {
        return Ok(None);
    };
    let contract_type_hash = match account_contract_code_hash(provider, &address).await {
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

    Ok(Some(ValidatorSourceDto::new(address, contract_type_hash)))
}

async fn scan_proxy_source_address(
    provider: &ValidatorSourceProvider,
    proxy_wallet: &str,
) -> Result<Option<String>> {
    let mut continuation_lt = None::<String>;

    for _ in 0..PROXY_SOURCE_TX_SCAN_MAX_PAGES {
        let tx_bocs = provider
            .transaction_bocs(
                proxy_wallet,
                continuation_lt.as_deref(),
                PROXY_SOURCE_TX_SCAN_LIMIT,
            )
            .await?;
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

        if next_continuation.as_deref() == Some("0") || next_continuation == continuation_lt {
            break;
        }
        continuation_lt = next_continuation;
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
