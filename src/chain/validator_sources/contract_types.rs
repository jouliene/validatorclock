use super::VALIDATOR_TYPE_FETCH_CONCURRENCY;
use crate::config::ChainConfig;
use anyhow::{Context, Result, bail};
use minik2::Transport;
use tokio::task::JoinSet;
use tracing::{debug, warn};
use tycho_types::models::account::AccountState;

pub(super) async fn fetch_validator_contract_type_hashes(
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

pub(super) async fn account_contract_code_hash(
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
