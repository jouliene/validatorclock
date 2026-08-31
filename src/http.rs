use reqwest::Client;
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
