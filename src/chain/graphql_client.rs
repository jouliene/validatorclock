use super::rpc_retry::{RpcCallError, retry_transient_call};
use super::util::endpoint_label;
use anyhow::{Context, Result, anyhow, bail};
use reqwest::{Client, StatusCode, Url};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::env;
use tokio::time::Duration;

const GRAPHQL_REQUEST_TIMEOUT: Duration = Duration::from_secs(25);
const GRAPHQL_API_KEY_ENV: &str = "VALIDATORCLOCK_GRAPHQL_API_KEY";
const GRAPHQL_ERROR_BODY_LIMIT: usize = 400;

#[derive(Debug, Clone)]
pub(super) struct GraphqlClient {
    client: Client,
    endpoint: String,
    label: String,
    api_key: Option<String>,
}

impl GraphqlClient {
    pub(super) fn new(endpoint: &str) -> Result<Self> {
        let endpoint = endpoint.trim();
        if endpoint.is_empty() {
            bail!("GraphQL endpoint is empty");
        }
        Url::parse(endpoint).context("invalid GraphQL endpoint URL")?;

        Ok(Self {
            client: crate::http::shared_client().clone(),
            endpoint: endpoint.to_owned(),
            label: endpoint_label(endpoint),
            api_key: env::var(GRAPHQL_API_KEY_ENV)
                .ok()
                .map(|key| key.trim().to_owned())
                .filter(|key| !key.is_empty()),
        })
    }

    pub(super) async fn query<R>(&self, name: &str, query: &str, variables: Value) -> Result<R>
    where
        R: DeserializeOwned,
    {
        retry_transient_call("GraphQL request did not run", || {
            self.query_once(name, query, &variables)
        })
        .await
    }

    async fn query_once<R>(
        &self,
        name: &str,
        query: &str,
        variables: &Value,
    ) -> Result<R, RpcCallError>
    where
        R: DeserializeOwned,
    {
        let mut request = json!({ "query": query });
        if !variables.is_null() {
            request["variables"] = variables.clone();
        }

        let mut builder = self
            .client
            .post(&self.endpoint)
            .timeout(GRAPHQL_REQUEST_TIMEOUT)
            .json(&request);
        if let Some(api_key) = &self.api_key {
            builder = builder.header("Authorization", format!("Bearer {api_key}"));
        }

        // Queries are read-only, so a connection that never produced a response
        // is retried instead of failing the whole refresh on one bad second.
        // The endpoint carries the project id in its path, and this message
        // reaches the public /api/status and the clock warning. A reqwest
        // error prints the URL it failed on, which would put the id there in
        // full, right beside the label that exists to mask it.
        let response = builder.send().await.map_err(|error| {
            RpcCallError::Transient(anyhow!(
                "failed to send GraphQL `{name}` request to {}: {}",
                self.label,
                error.without_url()
            ))
        })?;
        let status = response.status();
        let body = response.text().await.map_err(|error| {
            RpcCallError::Transient(anyhow!(
                "failed to read GraphQL `{name}` response from {}: {}",
                self.label,
                error.without_url()
            ))
        })?;

        if !status.is_success() {
            let error = anyhow!(
                "GraphQL HTTP error {status} for `{name}` at {}: {}",
                self.label,
                truncate_body(&body)
            );
            return if is_retryable_status(status) {
                Err(RpcCallError::Transient(error))
            } else {
                Err(RpcCallError::Other(error))
            };
        }

        let value = serde_json::from_str::<Value>(&body).map_err(|error| {
            RpcCallError::Other(anyhow!(
                "failed to parse GraphQL `{name}` response from {}: {error}",
                self.label
            ))
        })?;

        if let Some(message) = graphql_errors_message(&value) {
            let error = anyhow!("GraphQL error for `{name}` at {}: {message}", self.label);
            return if is_rate_limit_message(&message) {
                Err(RpcCallError::Transient(error))
            } else {
                Err(RpcCallError::Other(error))
            };
        }

        let data = value.get("data").cloned().ok_or_else(|| {
            RpcCallError::Other(anyhow!(
                "GraphQL `{name}` response from {} has no data field",
                self.label
            ))
        })?;

        serde_json::from_value(data).map_err(|error| {
            RpcCallError::Other(anyhow!(
                "failed to deserialize GraphQL `{name}` data from {}: {error}",
                self.label
            ))
        })
    }
}

pub(super) fn is_graphql_endpoint(endpoint: &str) -> bool {
    let Ok(url) = Url::parse(endpoint.trim()) else {
        return false;
    };

    url.path_segments()
        .and_then(|mut segments| segments.rfind(|segment| !segment.is_empty()))
        .is_some_and(|segment| segment.eq_ignore_ascii_case("graphql"))
}

fn graphql_errors_message(value: &Value) -> Option<String> {
    let errors = value.get("errors")?.as_array()?;
    if errors.is_empty() {
        return None;
    }

    let message = errors
        .iter()
        .map(|error| {
            error
                .get("message")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .unwrap_or_else(|| error.to_string())
        })
        .collect::<Vec<_>>()
        .join("; ");
    Some(truncate_body(&message))
}

fn is_rate_limit_message(message: &str) -> bool {
    let message = message.to_ascii_lowercase();
    message.contains("rate limit")
        || message.contains("too many requests")
        || message.contains("quota")
}

fn is_retryable_status(status: StatusCode) -> bool {
    status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
}

fn truncate_body(body: &str) -> String {
    let body = body.trim();
    if body.len() <= GRAPHQL_ERROR_BODY_LIMIT {
        return body.to_owned();
    }

    let cut = body
        .char_indices()
        .map(|(index, _)| index)
        .take_while(|index| *index <= GRAPHQL_ERROR_BODY_LIMIT)
        .last()
        .unwrap_or(0);
    format!("{}…", &body[..cut])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_graphql_endpoints() {
        assert!(is_graphql_endpoint(
            "https://mainnet.evercloud.dev/project/graphql"
        ));
        assert!(is_graphql_endpoint(
            "https://mainnet.evercloud.dev/project/graphql/"
        ));
        assert!(is_graphql_endpoint("https://example.com/GraphQL"));
        assert!(!is_graphql_endpoint("https://jrpc.everwallet.net"));
        assert!(!is_graphql_endpoint("https://toncenter.com/api/v2/jsonRPC"));
        assert!(!is_graphql_endpoint("https://example.com/mygraphql"));
        assert!(!is_graphql_endpoint("not a url"));
    }

    #[test]
    fn collects_graphql_error_messages() {
        let value = json!({
            "errors": [{ "message": "Cannot query field" }, { "message": "second" }]
        });

        assert_eq!(
            graphql_errors_message(&value).unwrap(),
            "Cannot query field; second"
        );
        assert!(graphql_errors_message(&json!({ "data": {} })).is_none());
        assert!(graphql_errors_message(&json!({ "errors": [] })).is_none());
    }

    #[test]
    fn detects_rate_limit_messages() {
        assert!(is_rate_limit_message("Rate limit exceeded"));
        assert!(is_rate_limit_message("Too Many Requests"));
        assert!(!is_rate_limit_message("Cannot query field"));
    }

    #[test]
    fn truncates_long_error_bodies() {
        let body = "x".repeat(GRAPHQL_ERROR_BODY_LIMIT + 100);

        let truncated = truncate_body(&body);

        assert!(truncated.len() < body.len());
        assert!(truncated.ends_with('…'));
    }

    /// The project id lives in the endpoint path, and this client's messages
    /// reach the unauthenticated /api/status and the clock warning. A reqwest
    /// error prints the URL it failed on, so the error must not be formatted
    /// as it comes - the masked label is the only form of the endpoint that
    /// may travel.
    #[tokio::test]
    async fn a_failed_request_does_not_carry_the_endpoint_secret() {
        const SECRET: &str = "0123456789abcdef0123456789abcdef";

        // A port nothing listens on: the request is refused at once.
        let closed_port = {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let port = listener.local_addr().unwrap().port();
            drop(listener);
            port
        };
        let client =
            GraphqlClient::new(&format!("http://127.0.0.1:{closed_port}/{SECRET}/graphql"))
                .unwrap();

        let error = client
            .query::<Value>("test", "{ info { version } }", Value::Null)
            .await
            .expect_err("a refused request should fail");
        let message = format!("{error:?}");

        assert!(
            !message.contains(SECRET),
            "the endpoint secret must not reach the message: {message}"
        );
        assert!(
            message.contains("***"),
            "the masked label should still name the endpoint: {message}"
        );
    }
}
