use super::rpc_retry::{RpcCallError, retry_transient_call};
use anyhow::{Result, anyhow, bail};
use reqwest::{Client, StatusCode, Url};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::env;
use tokio::time::Duration;

const TONCENTER_REQUEST_TIMEOUT: Duration = Duration::from_secs(25);

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
            client: crate::http::shared_client().clone(),
            endpoint: endpoint.to_owned(),
            api_key: is_toncenter_own_host(endpoint)
                .then(|| env::var("VALIDATORCLOCK_TONCENTER_API_KEY").ok())
                .flatten(),
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
        let mut builder = self
            .client
            .post(&self.endpoint)
            .timeout(TONCENTER_REQUEST_TIMEOUT)
            .json(&request);
        if let Some(api_key) = &self.api_key {
            builder = builder.header("X-API-Key", api_key);
        }

        // This message reaches the public /api/status and the clock warning,
        // and a reqwest error prints the URL it failed on - which may carry a
        // key in its path or query.
        let response = builder.send().await.map_err(|error| {
            RpcCallError::Transient(anyhow!(
                "failed to send TON Center `{method}` request: {}",
                error.without_url()
            ))
        })?;
        let status = response.status();
        let value = response.json::<Value>().await.map_err(|error| {
            RpcCallError::Transient(anyhow!(
                "failed to parse TON Center `{method}` response: {}",
                error.without_url()
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

/// An endpoint that speaks TON Center's v2 JSON-RPC dialect.
///
/// The dialect is not TON Center's alone. Orbs TON Access serves the same
/// methods at the same `/jsonRPC` path, and so does a self-hosted
/// `ton-http-api`; TON Center's own host has been rate limiting an
/// unauthenticated caller after two requests, so being able to name another
/// one is the difference between having a fallback and not having one.
///
/// What the path cannot say is who is on the other end. That question is
/// answered separately, by `is_toncenter_own_host`, and only for deciding
/// where the API key may go.
pub(super) fn is_toncenter_json_rpc_endpoint(endpoint: &str) -> bool {
    let Ok(url) = Url::parse(endpoint.trim()) else {
        return false;
    };

    url.path_segments()
        .and_then(|mut segments| segments.rfind(|segment| !segment.is_empty()))
        .is_some_and(|segment| segment.eq_ignore_ascii_case("jsonRPC"))
}

/// TON Center's own hosts - the only ones the API key is sent to.
///
/// The key travels in a request header, so it goes wherever the endpoint
/// points. Now that an endpoint speaking this dialect may belong to someone
/// else, handing them the key along with the request would be giving away a
/// credential to a party that never asked for one.
fn is_toncenter_own_host(endpoint: &str) -> bool {
    let Ok(url) = Url::parse(endpoint.trim()) else {
        return false;
    };
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
        // Someone else serving the same dialect. Recognising it is the whole
        // point: TON Center alone leaves the chain without a fallback.
        assert!(is_toncenter_json_rpc_endpoint(
            "https://ton.access.orbs.network/4411/1/mainnet/toncenter-api-v2/jsonRPC"
        ));

        // The other two dialects must not be mistaken for this one.
        assert!(!is_toncenter_json_rpc_endpoint(
            "https://jrpc-ton.broxus.com"
        ));
        assert!(!is_toncenter_json_rpc_endpoint(
            "https://mainnet.evercloud.dev/graphql"
        ));
    }

    #[test]
    fn the_api_key_goes_only_to_toncenter_itself() {
        assert!(is_toncenter_own_host(
            "https://toncenter.com/api/v2/jsonRPC"
        ));
        assert!(is_toncenter_own_host(
            "https://testnet.toncenter.com/api/v2/jsonRPC"
        ));

        // Speaks the same dialect, but the key is not theirs to hold.
        assert!(!is_toncenter_own_host(
            "https://ton.access.orbs.network/4411/1/mainnet/toncenter-api-v2/jsonRPC"
        ));
        // And a host that merely says `toncenter.com` inside its path is not
        // TON Center either.
        assert!(!is_toncenter_own_host(
            "https://example.com/toncenter.com/api/v2/jsonRPC"
        ));
    }
}
