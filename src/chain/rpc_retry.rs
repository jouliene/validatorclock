use anyhow::{Result, anyhow};
use std::future::Future;
use tokio::time::{Duration, sleep};

const RPC_MAX_ATTEMPTS: usize = 3;
const RPC_RETRY_DELAY: Duration = Duration::from_millis(1_500);

#[derive(Debug)]
pub(super) enum RpcCallError {
    Transient(anyhow::Error),
    Other(anyhow::Error),
}

pub(super) async fn retry_transient_call<T, F, Fut>(
    empty_error: &'static str,
    mut call_once: F,
) -> Result<T>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, RpcCallError>>,
{
    let mut last_error = None;
    for attempt in 1..=RPC_MAX_ATTEMPTS {
        match call_once().await {
            Ok(result) => return Ok(result),
            Err(RpcCallError::Transient(error)) if attempt < RPC_MAX_ATTEMPTS => {
                last_error = Some(error);
                sleep(RPC_RETRY_DELAY).await;
            }
            Err(RpcCallError::Transient(error) | RpcCallError::Other(error)) => {
                return Err(error);
            }
        }
    }

    Err(last_error.unwrap_or_else(|| anyhow!(empty_error)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[tokio::test]
    async fn returns_first_success() {
        let calls = Cell::new(0);

        let result: Result<u8> = retry_transient_call("did not run", || {
            calls.set(calls.get() + 1);
            async { Ok(7) }
        })
        .await;

        assert_eq!(result.unwrap(), 7);
        assert_eq!(calls.get(), 1);
    }

    #[tokio::test]
    async fn stops_on_non_transient_error() {
        let calls = Cell::new(0);

        let result: Result<u8> = retry_transient_call("did not run", || {
            calls.set(calls.get() + 1);
            async { Err(RpcCallError::Other(anyhow!("fatal"))) }
        })
        .await;

        assert!(result.unwrap_err().to_string().contains("fatal"));
        assert_eq!(calls.get(), 1);
    }
}
