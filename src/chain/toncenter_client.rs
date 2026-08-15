use super::rpc_retry::{RpcCallError, retry_transient_call};
use anyhow::{Context, Result, anyhow, bail};
use reqwest::{Client, StatusCode, Url};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::env;
use tokio::time::Duration;

#[derive(Debug, Clone)]
pub(super) struct TonCenterJsonRpcClient {
    client: Client,
    endpoint: String,
    api_key: Option<String>,
}

impl TonCenterJsonRpcClient {
    pub(super) fn new(endpoint: &str) -> Result<Self> {
        let endpoint = endpoint.trim();
        if endpoint.is_empty() {
            bail!("TON Center endpoint is empty");
        }

        Ok(Self {
            client: Client::builder()
                .connect_timeout(Duration::from_secs(5))
                .timeout(Duration::from_secs(25))
                .build()
                .context("failed to build TON Center HTTP client")?,
            endpoint: endpoint.to_owned(),
            api_key: env::var("VALIDATORCLOCK_TONCENTER_API_KEY").ok(),
        })
    }

    pub(super) fn http_client(&self) -> &Client {
        &self.client
    }

    pub(super) fn api_key(&self) -> Option<&str> {
        self.api_key.as_deref()
    }

    pub(super) async fn call<R>(&self, method: &str, params: Value) -> Result<R>
    where
        R: DeserializeOwned,
    {
        retry_transient_call("TON Center request did not run", || {
            self.call_once(method, &params)
        })
        .await
    }

    async fn call_once<R>(&self, method: &str, params: &Value) -> Result<R, RpcCallError>
    where
        R: DeserializeOwned,
    {
        let request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": params,
        });
        let mut builder = self.client.post(&self.endpoint).json(&request);
        if let Some(api_key) = &self.api_key {
            builder = builder.header("X-API-Key", api_key);
        }

        let response = builder.send().await.map_err(|error| {
            RpcCallError::Transient(anyhow!(
                "failed to send TON Center `{method}` request: {error}"
            ))
        })?;
        let status = response.status();
        let value = response.json::<Value>().await.map_err(|error| {
            RpcCallError::Transient(anyhow!(
                "failed to parse TON Center `{method}` response: {error}"
            ))
        })?;

        if !status.is_success() {
            let error = anyhow!("TON Center HTTP error {status} for `{method}`: {value}");
            return if status == StatusCode::TOO_MANY_REQUESTS {
                Err(RpcCallError::Transient(error))
            } else {
                Err(RpcCallError::Other(error))
            };
        }

        let ok = value.get("ok").and_then(Value::as_bool).unwrap_or(false);
        if !ok {
            let code = value.get("code").and_then(Value::as_i64);
            let detail = value
                .get("error")
                .or_else(|| value.get("result"))
                .map(Value::to_string)
                .unwrap_or_else(|| "unknown error".to_owned());
            let error = anyhow!("TON Center error for `{method}`: code={code:?} {detail}");
            return if code == Some(429) {
                Err(RpcCallError::Transient(error))
            } else {
                Err(RpcCallError::Other(error))
            };
        }

        let result = value.get("result").cloned().ok_or_else(|| {
            RpcCallError::Other(anyhow!(
                "TON Center `{method}` response has no result field"
            ))
        })?;
        serde_json::from_value(result).map_err(|error| {
            RpcCallError::Other(anyhow!(
                "failed to deserialize TON Center `{method}` result: {error}"
            ))
        })
    }
}

pub(super) fn is_toncenter_json_rpc_endpoint(endpoint: &str) -> bool {
    let Ok(url) = Url::parse(endpoint.trim()) else {
        return false;
    };
    if url.path().trim_end_matches('/') != "/api/v2/jsonRPC" {
        return false;
    }

    let Some(host) = url.host_str() else {
        return false;
    };
    host == "toncenter.com"
        || host.ends_with(".toncenter.com")
        || (cfg!(test) && matches!(host, "localhost" | "127.0.0.1" | "::1"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_toncenter_json_rpc_endpoint() {
        assert!(is_toncenter_json_rpc_endpoint(
            "https://toncenter.com/api/v2/jsonRPC"
        ));
        assert!(is_toncenter_json_rpc_endpoint(
            "https://toncenter.com/api/v2/jsonRPC/"
        ));
        assert!(is_toncenter_json_rpc_endpoint(
            "https://testnet.toncenter.com/api/v2/jsonRPC"
        ));
        assert!(!is_toncenter_json_rpc_endpoint(
            "https://jrpc-ton.broxus.com"
        ));
        assert!(!is_toncenter_json_rpc_endpoint(
            "https://example.com/toncenter.com/api/v2/jsonRPC"
        ));
    }
}
