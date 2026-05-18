use super::VALIDATOR_TYPE_FETCH_CONCURRENCY;
use super::provider::ValidatorSourceProvider;
use anyhow::Result;
use tokio::task::JoinSet;
use tracing::{debug, warn};

pub(super) async fn fetch_validator_contract_type_hashes(
    chain_id: &str,
    provider: &ValidatorSourceProvider,
    wallets: Vec<String>,
) -> Result<Vec<(String, String)>> {
    if let Some(fetched) = provider.account_code_hashes(wallets.clone()).await? {
        return Ok(fetched);
    }

    let mut fetched = Vec::new();

    for chunk in wallets.chunks(VALIDATOR_TYPE_FETCH_CONCURRENCY) {
        let mut tasks = JoinSet::new();
        for wallet in chunk {
            let provider = provider.clone();
            let wallet = wallet.clone();
            tasks.spawn(async move {
                let result = account_contract_code_hash(&provider, &wallet).await;
                (wallet, result)
            });
        }

        while let Some(result) = tasks.join_next().await {
            match result {
                Ok((wallet, Ok(repr_hash))) => fetched.push((wallet, repr_hash)),
                Ok((wallet, Err(error))) => {
                    debug!(
                        chain_id = %chain_id,
                        wallet,
                        error = ?error,
                        "failed to fetch validator contract type hash"
                    );
                }
                Err(error) => {
                    warn!(
                        chain_id = %chain_id,
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
    provider: &ValidatorSourceProvider,
    account_address: &str,
) -> Result<String> {
    provider.account_code_hash(account_address).await
}
