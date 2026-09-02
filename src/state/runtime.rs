use super::AppState;
use serde::Serialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Default, Serialize)]
pub(super) struct ChainRuntimeStatus {
    pub(super) last_attempt_at: Option<u64>,
    pub(super) last_success_at: Option<u64>,
    pub(super) last_error: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ChainRuntimeSnapshot {
    pub(crate) cached: bool,
    pub(crate) fetched_at: Option<u64>,
    pub(crate) age_seconds: Option<u64>,
    pub(crate) stale: bool,
    pub(crate) last_attempt_at: Option<u64>,
    pub(crate) last_success_at: Option<u64>,
    pub(crate) last_error: Option<String>,
}

impl AppState {
    pub(crate) async fn chain_runtime_snapshots(
        &self,
        now: u64,
        refresh_seconds: u64,
    ) -> HashMap<String, ChainRuntimeSnapshot> {
        let cache = self.cache.read().await;
        let chain_status = self.chain_status.read().await;

        self.config
            .chains
            .iter()
            .map(|chain| {
                let cached = cache.get(&chain.id);
                let fetched_at = cached.map(|entry| entry.snapshot().fetched_at());
                let age_seconds = fetched_at.map(|fetched_at| now.saturating_sub(fetched_at));
                let stale = age_seconds.is_none_or(|age| age > refresh_seconds.saturating_mul(2));
                let status = chain_status.get(&chain.id);

                (
                    chain.id.clone(),
                    ChainRuntimeSnapshot {
                        cached: cached.is_some(),
                        fetched_at,
                        age_seconds,
                        stale,
                        last_attempt_at: status.and_then(|status| status.last_attempt_at),
                        last_success_at: status.and_then(|status| status.last_success_at),
                        last_error: status.and_then(|status| status.last_error.clone()),
                    },
                )
            })
            .collect()
    }

    pub(crate) async fn record_refresh_attempt(&self, chain_id: &str, at: u64) {
        let mut status = self.chain_status.write().await;
        status
            .entry(chain_id.to_owned())
            .or_default()
            .last_attempt_at = Some(at);
    }

    pub(crate) async fn record_refresh_success(&self, chain_id: &str, at: u64) {
        let mut status = self.chain_status.write().await;
        let status = status.entry(chain_id.to_owned()).or_default();
        status.last_attempt_at = Some(at);
        status.last_success_at = Some(at);
        status.last_error = None;
    }

    pub(crate) async fn record_refresh_failure(&self, chain_id: &str, at: u64, error: String) {
        let mut status = self.chain_status.write().await;
        let status = status.entry(chain_id.to_owned()).or_default();
        status.last_attempt_at = Some(at);
        status.last_error = Some(error);
    }

    pub(crate) async fn mark_refresh_attempt_if_due(
        &self,
        chain_id: &str,
        at: u64,
        retry_after_seconds: u64,
    ) -> bool {
        let mut status = self.chain_status.write().await;
        let status = status.entry(chain_id.to_owned()).or_default();
        if status
            .last_attempt_at
            .is_some_and(|last_attempt| at.saturating_sub(last_attempt) < retry_after_seconds)
        {
            return false;
        }
        status.last_attempt_at = Some(at);
        true
    }
}

/// The right to refresh one chain, held for as long as the refresh runs.
///
/// The guard that existed before was a timestamp: it recorded when a refresh
/// *started*, so once the retry window passed a second refresh could begin
/// beside one still running - and the background loop did not consult it at
/// all. Two refreshes of a chain then raced each other into the cache, and
/// whichever finished last won regardless of which had the newer data.
pub(crate) struct RefreshClaim {
    chains: std::sync::Arc<std::sync::Mutex<std::collections::HashSet<String>>>,
    chain_id: String,
}

impl Drop for RefreshClaim {
    fn drop(&mut self) {
        if let Ok(mut chains) = self.chains.lock() {
            chains.remove(&self.chain_id);
        }
    }
}

impl AppState {
    /// Claims a chain for refreshing, or reports that someone already holds
    /// it. The claim is released when the guard drops, including when the task
    /// holding it is cancelled.
    pub(crate) fn claim_refresh(&self, chain_id: &str) -> Option<RefreshClaim> {
        let mut chains = self.refreshing.lock().ok()?;
        if !chains.insert(chain_id.to_owned()) {
            return None;
        }
        Some(RefreshClaim {
            chains: std::sync::Arc::clone(&self.refreshing),
            chain_id: chain_id.to_owned(),
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::config::{AppConfig, ChainConfig};
    use crate::state::AppState;
    use std::sync::Arc;

    fn test_state() -> AppState {
        AppState::new(Arc::new(AppConfig::for_test(vec![ChainConfig {
            id: "test".to_owned(),
            name: "Test".to_owned(),
            rpc: "https://example.com".to_owned(),
            rpc_fallbacks: Vec::new(),
            color: "#38bdf8".to_owned(),
            token_symbol: "TEST".to_owned(),
            rpc_label: None,
        }])))
    }

    /// The guard before this was a timestamp recording when a refresh started,
    /// so once the retry window passed a second could begin beside one still
    /// running - and the background tick never consulted it at all.
    #[test]
    fn one_refresh_of_a_chain_at_a_time() {
        let state = test_state();

        let held = state.claim_refresh("test").expect("the first claim wins");
        assert!(
            state.claim_refresh("test").is_none(),
            "a second refresh of the same chain must not start"
        );
        assert!(
            state.claim_refresh("other").is_some(),
            "another chain is not held up by it"
        );

        drop(held);
        assert!(
            state.claim_refresh("test").is_some(),
            "the claim is released when its holder goes, cancelled or not"
        );
    }
}
