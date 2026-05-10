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
