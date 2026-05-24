use crate::chain::toncenter_client::{
    TonCenterCallError, TonCenterJsonRpcClient, retry_toncenter_call,
};
use anyhow::{Context, Result, anyhow, bail};
use reqwest::{StatusCode, Url};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tycho_types::boc::Boc;
use tycho_types::cell::Cell;

#[derive(Clone)]
pub(in crate::chain::validator_sources) struct TonCenterValidatorSourceProvider {
    client: TonCenterJsonRpcClient,
    account_states_endpoint: String,
    account_states: Arc<Mutex<HashMap<String, TonCenterAccountState>>>,
}

impl TonCenterValidatorSourceProvider {
    pub(super) fn new(endpoint: &str) -> Result<Self> {
        Ok(Self {
            client: TonCenterJsonRpcClient::new(endpoint)?,
            account_states_endpoint: account_states_endpoint(endpoint),
            account_states: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    pub(super) async fn account_code_hash(&self, account_address: &str) -> Result<String> {
        let state = self.get_account_state(account_address).await?;
        code_hash_from_toncenter_state(&state, account_address)
    }

    pub(super) async fn account_code_hashes(
        &self,
        account_addresses: Vec<String>,
    ) -> Result<Vec<(String, String)>> {
        self.load_account_states(&account_addresses).await?;
        let cache = self.account_states.lock().await;
        Ok(account_addresses
            .into_iter()
            .filter_map(|account_address| {
                cache.get(&address_key(&account_address)).and_then(|state| {
                    code_hash_from_toncenter_state(state, &account_address)
                        .ok()
                        .map(|hash| (account_address, hash))
                })
            })
            .collect())
    }

    pub(super) async fn account_data(&self, account_address: &str) -> Result<Cell> {
        let state = self.get_account_state(account_address).await?;
        let data = state
            .data_boc
            .as_deref()
            .filter(|data| !data.is_empty())
            .with_context(|| format!("TON Center account `{account_address}` has no data"))?;
        Boc::decode_base64(data).with_context(|| {
            format!("failed to decode TON Center account data `{account_address}`")
        })
    }

    async fn get_account_state(&self, account_address: &str) -> Result<TonCenterAccountState> {
        self.load_account_states(&[account_address.to_owned()])
            .await?;
        let cache = self.account_states.lock().await;
        let state = cache
            .get(&address_key(account_address))
            .with_context(|| format!("TON Center account `{account_address}` not found"))?;
        ensure_active_toncenter_account(state, account_address)?;
        Ok(state.clone())
    }

    async fn load_account_states(&self, account_addresses: &[String]) -> Result<()> {
        let missing = {
            let cache = self.account_states.lock().await;
            account_addresses
                .iter()
                .filter(|address| !cache.contains_key(&address_key(address)))
                .cloned()
                .collect::<Vec<_>>()
        };
        if missing.is_empty() {
            return Ok(());
        }

        let states = self.fetch_account_states(&missing).await?;
        let mut cache = self.account_states.lock().await;
        for state in states {
            cache.insert(address_key(&state.address), state);
        }
        Ok(())
    }

    async fn fetch_account_states(
        &self,
        account_addresses: &[String],
    ) -> Result<Vec<TonCenterAccountState>> {
        retry_toncenter_call("TON Center account states request did not run", || {
            self.fetch_account_states_once(account_addresses)
        })
        .await
    }

    async fn fetch_account_states_once(
        &self,
        account_addresses: &[String],
    ) -> Result<Vec<TonCenterAccountState>, TonCenterCallError> {
        let mut url = Url::parse(&self.account_states_endpoint).map_err(|error| {
            TonCenterCallError::Other(anyhow!(
                "invalid TON Center account states endpoint `{}`: {error}",
                self.account_states_endpoint
            ))
        })?;
        {
            let mut query = url.query_pairs_mut();
            query.append_pair("include_boc", "true");
            for account_address in account_addresses {
                query.append_pair("address", account_address);
            }
        }
        let mut builder = self.client.http_client().get(url);
        if let Some(api_key) = self.client.api_key() {
            builder = builder.header("X-API-Key", api_key);
        }

        let response = builder.send().await.map_err(|error| {
            TonCenterCallError::Other(anyhow!(
                "failed to send TON Center account states request: {error}"
            ))
        })?;
        let status = response.status();
        let value = response.json::<Value>().await.map_err(|error| {
            TonCenterCallError::Other(anyhow!(
                "failed to parse TON Center account states response: {error}"
            ))
        })?;

        if !status.is_success() {
            let error = anyhow!("TON Center account states HTTP error {status}: {value}");
            return if status == StatusCode::TOO_MANY_REQUESTS {
                Err(TonCenterCallError::RateLimited(error))
            } else {
                Err(TonCenterCallError::Other(error))
            };
        }

        serde_json::from_value::<TonCenterAccountStatesResponse>(value)
            .map(|response| response.accounts)
            .map_err(|error| {
                TonCenterCallError::Other(anyhow!(
                    "failed to deserialize TON Center account states response: {error}"
                ))
            })
    }

    pub(super) async fn transaction_bocs(
        &self,
        account_address: &str,
        continuation_lt: Option<&str>,
        limit: u8,
    ) -> Result<Vec<String>> {
        let mut params = json!({
            "address": account_address,
            "limit": limit,
        });
        if let Some(lt) = continuation_lt {
            params["to_lt"] = json!(lt);
        }

        let transactions: Vec<TonCenterTransaction> = self.call("getTransactions", params).await?;
        Ok(transactions
            .into_iter()
            .filter_map(|transaction| (!transaction.data.is_empty()).then_some(transaction.data))
            .collect())
    }

    async fn call<R>(&self, method: &str, params: Value) -> Result<R>
    where
        R: serde::de::DeserializeOwned,
    {
        self.client.call(method, params).await
    }
}

fn code_hash_from_toncenter_state(
    state: &TonCenterAccountState,
    account_address: &str,
) -> Result<String> {
    ensure_active_toncenter_account(state, account_address)?;
    let code = state
        .code_boc
        .as_deref()
        .filter(|code| !code.is_empty())
        .with_context(|| format!("TON Center account `{account_address}` has no code"))?;
    let code = Boc::decode_base64(code)
        .with_context(|| format!("failed to decode TON Center account code `{account_address}`"))?;
    Ok(code.repr_hash().to_string())
}

fn ensure_active_toncenter_account(
    state: &TonCenterAccountState,
    account_address: &str,
) -> Result<()> {
    if state.status.as_deref() != Some("active") {
        bail!("TON Center account `{account_address}` is not active");
    }
    Ok(())
}

fn account_states_endpoint(endpoint: &str) -> String {
    endpoint
        .trim_end_matches('/')
        .replace("/api/v2/jsonRPC", "/api/v3/accountStates")
}

fn address_key(address: &str) -> String {
    address.to_ascii_lowercase()
}

#[derive(Debug, Deserialize)]
struct TonCenterAccountStatesResponse {
    accounts: Vec<TonCenterAccountState>,
}

#[derive(Debug, Clone, Deserialize)]
struct TonCenterAccountState {
    address: String,
    status: Option<String>,
    code_boc: Option<String>,
    data_boc: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TonCenterTransaction {
    data: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_v3_account_states_endpoint() {
        assert_eq!(
            account_states_endpoint("https://toncenter.com/api/v2/jsonRPC"),
            "https://toncenter.com/api/v3/accountStates"
        );
        assert_eq!(
            account_states_endpoint("https://toncenter.com/api/v2/jsonRPC/"),
            "https://toncenter.com/api/v3/accountStates"
        );
    }

    #[test]
    fn normalizes_account_state_cache_keys() {
        assert_eq!(address_key("-1:EF7EBD"), "-1:ef7ebd");
    }
}
