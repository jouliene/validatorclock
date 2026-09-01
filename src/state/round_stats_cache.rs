//! A short-lived hold on the last round-stats answer per chain.
//!
//! The public endpoint fetches live unless the caller asks for cache, and one
//! answer costs several sequential upstream calls. Without this, a hundred
//! requests arriving together produced a hundred fan-outs against a metered
//! third-party API. Now the first one fetches while the rest wait on the same
//! lock, and by the time they hold it the answer is already there.

use super::AppState;
use crate::chain::ChainRoundStatsDto;
use crate::timeutil::now_sec;

/// Round statistics change once a round, which is hours. Seconds of hold cost
/// nothing in freshness and remove the fan-out entirely.
const HOLD_SECONDS: u64 = 30;

#[derive(Debug, Default)]
pub(super) struct RoundStatsHold {
    entries: std::collections::HashMap<String, HeldStats>,
}

#[derive(Debug, Clone)]
struct HeldStats {
    stats: ChainRoundStatsDto,
    stored_at: u64,
}

impl AppState {
    /// The lock a caller must hold to fetch round stats for a chain, so that
    /// concurrent callers queue instead of all going upstream.
    pub(crate) async fn round_stats_fetch_guard(
        &self,
        chain_id: &str,
    ) -> tokio::sync::OwnedMutexGuard<()> {
        let lock = {
            let mut locks = self.round_stats_locks.lock().await;
            std::sync::Arc::clone(
                locks
                    .entry(chain_id.to_owned())
                    .or_insert_with(|| std::sync::Arc::new(tokio::sync::Mutex::new(()))),
            )
        };
        lock.lock_owned().await
    }

    pub(crate) async fn held_round_stats(&self, chain_id: &str) -> Option<ChainRoundStatsDto> {
        let now = now_sec();
        let hold = self.round_stats_hold.read().await;
        hold.entries
            .get(chain_id)
            .filter(|held| now.saturating_sub(held.stored_at) < HOLD_SECONDS)
            .map(|held| held.stats.clone())
    }

    pub(crate) async fn hold_round_stats(&self, chain_id: &str, stats: &ChainRoundStatsDto) {
        let mut hold = self.round_stats_hold.write().await;
        hold.entries.insert(
            chain_id.to_owned(),
            HeldStats {
                stats: stats.clone(),
                stored_at: now_sec(),
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppConfig;
    use std::sync::Arc;

    fn test_state() -> Arc<AppState> {
        Arc::new(AppState::new(Arc::new(AppConfig::for_test(vec![
            crate::config::ChainConfig {
                id: "test".to_owned(),
                name: "Test".to_owned(),
                rpc: "https://example.com".to_owned(),
                rpc_fallbacks: Vec::new(),
                color: "#38bdf8".to_owned(),
                token_symbol: "TEST".to_owned(),
                rpc_label: None,
            },
        ]))))
    }

    /// One answer costs several sequential upstream calls, so a hundred
    /// requests arriving together used to mean a hundred fan-outs against a
    /// metered API. The second caller must find the first one's answer.
    #[tokio::test]
    async fn an_answer_is_held_briefly_for_the_next_caller() {
        let state = test_state();
        assert!(state.held_round_stats("test").await.is_none());

        let stats = crate::chain::test_round_stats("test");
        state.hold_round_stats("test", &stats).await;

        assert!(state.held_round_stats("test").await.is_some());
        assert!(
            state.held_round_stats("other").await.is_none(),
            "one chain's answer must not stand in for another's"
        );
    }

    /// The guard is what makes the callers queue rather than all going
    /// upstream at once.
    #[tokio::test]
    async fn callers_for_one_chain_queue_behind_each_other() {
        let state = test_state();
        let held = state.round_stats_fetch_guard("test").await;

        let waiting = {
            let state = Arc::clone(&state);
            tokio::spawn(async move { state.round_stats_fetch_guard("test").await })
        };
        tokio::task::yield_now().await;
        assert!(
            !waiting.is_finished(),
            "the second caller should be waiting"
        );

        // A different chain is not held up by it.
        let other = tokio::time::timeout(
            std::time::Duration::from_millis(200),
            state.round_stats_fetch_guard("other"),
        )
        .await;
        assert!(
            other.is_ok(),
            "another chain should not queue behind this one"
        );

        drop(held);
        assert!(
            tokio::time::timeout(std::time::Duration::from_secs(2), waiting)
                .await
                .is_ok(),
            "the second caller should proceed once the first is done"
        );
    }
}
