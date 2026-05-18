use super::{VALIDATOR_TYPE_FETCH_CONCURRENCY, provider::ValidatorSourceProvider};
use anyhow::Result;
use std::future::Future;
use tokio::task::JoinSet;
use tracing::{debug, warn};

pub(super) async fn fetch_wallet_tasks<T, F, Fut>(
    chain_id: &str,
    provider: &ValidatorSourceProvider,
    wallets: Vec<String>,
    not_found_message: Option<&'static str>,
    error_message: &'static str,
    task_failed_message: &'static str,
    fetch: F,
) -> Vec<(String, T)>
where
    T: Send + 'static,
    F: Fn(ValidatorSourceProvider, String) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = Result<Option<T>>> + Send + 'static,
{
    let mut fetched = Vec::new();

    for chunk in wallets.chunks(VALIDATOR_TYPE_FETCH_CONCURRENCY) {
        let mut tasks = JoinSet::new();
        for wallet in chunk {
            let fetch = fetch.clone();
            let provider = provider.clone();
            let wallet = wallet.clone();
            tasks.spawn(async move {
                let result = fetch(provider, wallet.clone()).await;
                (wallet, result)
            });
        }

        while let Some(result) = tasks.join_next().await {
            match result {
                Ok((wallet, Ok(Some(value)))) => fetched.push((wallet, value)),
                Ok((wallet, Ok(None))) => {
                    if let Some(message) = not_found_message {
                        debug!(
                            chain_id = %chain_id,
                            wallet,
                            "{message}"
                        );
                    }
                }
                Ok((wallet, Err(error))) => {
                    debug!(
                        chain_id = %chain_id,
                        wallet,
                        error = ?error,
                        "{error_message}"
                    );
                }
                Err(error) => {
                    warn!(
                        chain_id = %chain_id,
                        error = ?error,
                        "{task_failed_message}"
                    );
                }
            }
        }
    }

    fetched
}
