use anyhow::{Context, Result, bail};
use reqwest::Client;
use serde::de::DeserializeOwned;
use std::sync::LazyLock;
use std::time::Duration;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const POOL_IDLE_TIMEOUT: Duration = Duration::from_secs(90);

/// One client for every outbound call, so the connection pool and the TLS
/// sessions it holds survive between refresh cycles instead of being rebuilt
/// per RPC call. Request timeouts are set per call by the caller.
pub(crate) fn shared_client() -> &'static Client {
    static CLIENT: LazyLock<Client> = LazyLock::new(|| {
        Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .pool_idle_timeout(POOL_IDLE_TIMEOUT)
            .build()
            .expect("HTTP client builder uses valid defaults")
    });
    &CLIENT
}

/// The most a third-party answer may weigh. A geo lookup returns a handful of
/// short fields per address; anything approaching this is not that.
pub(crate) const MAX_GEO_RESPONSE_BYTES: usize = 4 * 1024 * 1024;

/// Reads a JSON answer without letting the peer decide how much memory it
/// costs. `Response::json` buffers whatever arrives until the request timeout,
/// so a body that simply keeps coming is bounded only by the link speed.
pub(crate) async fn json_within<T: DeserializeOwned>(
    response: reqwest::Response,
    limit: usize,
) -> Result<T> {
    if response
        .content_length()
        .is_some_and(|length| length > limit as u64)
    {
        bail!("response declares more than {limit} bytes");
    }

    let mut body = Vec::new();
    let mut response = response;
    while let Some(chunk) = response
        .chunk()
        .await
        // A reqwest error prints the URL it failed on, and these URLs carry
        // tokens; callers put this text in logs and in the public API.
        .map_err(|error| error.without_url())
        .context("failed to read the response")?
    {
        if body.len() + chunk.len() > limit {
            bail!("response is longer than {limit} bytes");
        }
        body.extend_from_slice(&chunk);
    }

    serde_json::from_slice(&body).context("failed to decode the response")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shared_client_is_built_once() {
        let first = shared_client();
        let second = shared_client();

        assert!(std::ptr::eq(first, second));
    }
}
