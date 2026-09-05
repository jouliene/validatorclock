use super::super::ValidatorSourceDto;
use super::contract_types::account_contract_code_hash;
use super::provider::ValidatorSourceProvider;
use super::wallet_tasks::fetch_wallet_tasks;
use anyhow::Result;
use std::sync::OnceLock;
use tracing::debug;
use tycho_types::abi::{AbiValue, AbiVersion, FromAbi, Function, WithAbiType};
use tycho_types::models::{ComputePhase, MsgInfo, StdAddr, Transaction, TxInfo};

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
    super::transaction_scan::scan_back_for_source(
        provider,
        proxy_wallet,
        parse_proxy_process_new_stake_source,
    )
    .await
}

fn parse_proxy_process_new_stake_source(transaction: &Transaction) -> Result<Option<String>> {
    // Any message sent to a live account produces a transaction, whether the
    // contract accepted it or threw it out. Without this, anyone could send a
    // few cents to a validator's proxy with a body that merely decodes, and
    // the site would then present their address as the source of that
    // validator's stake - and keep presenting it, because a resolved source is
    // never looked at again.
    if !transaction_succeeded(transaction)? {
        return Ok(None);
    }

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

/// Whether the contract actually acted on the message, rather than merely
/// being handed it.
pub(super) fn transaction_succeeded(transaction: &Transaction) -> Result<bool> {
    let TxInfo::Ordinary(info) = transaction.load_info()? else {
        return Ok(false);
    };
    if info.aborted {
        return Ok(false);
    }

    Ok(match info.compute_phase {
        ComputePhase::Executed(executed) => executed.success,
        ComputePhase::Skipped(_) => false,
    })
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
