use super::provider::ValidatorSourceProvider;
use super::wallet_tasks::fetch_wallet_tasks;
use anyhow::Result;

pub(super) async fn fetch_validator_contract_type_hashes(
    chain_id: &str,
    provider: &ValidatorSourceProvider,
    wallets: Vec<String>,
) -> Result<Vec<(String, String)>> {
    if let Some(fetched) = provider.account_code_hashes(wallets.clone()).await? {
        return Ok(fetched);
    }

    Ok(fetch_wallet_tasks(
        chain_id,
        provider,
        wallets,
        None,
        "failed to fetch validator contract type hash",
        "validator contract type hash task failed",
        |provider, wallet| async move {
            account_contract_code_hash(&provider, &wallet)
                .await
                .map(Some)
        },
    )
    .await)
}

pub(super) async fn account_contract_code_hash(
    provider: &ValidatorSourceProvider,
    account_address: &str,
) -> Result<String> {
    provider.account_code_hash(account_address).await
}
