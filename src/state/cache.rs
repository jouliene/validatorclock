use super::AppState;
use crate::chain::{CacheEntry, ClockSnapshot, apply_cached_validator_types_to_snapshot};

impl AppState {
    pub(crate) async fn cached_snapshot_if_fresh(
        &self,
        chain_id: &str,
        now: u64,
        refresh_seconds: u64,
    ) -> Option<ClockSnapshot> {
        let mut snapshot = {
            let cache = self.cache.read().await;
            let entry = cache.get(chain_id)?;
            (now.saturating_sub(entry.fetched_at()) < refresh_seconds)
                .then(|| entry.snapshot().clone())?
        };
        self.annotate_snapshot(chain_id, &mut snapshot).await;
        apply_cached_validator_types_to_snapshot(self, chain_id, &mut snapshot).await;
        Some(snapshot)
    }

    pub(crate) async fn cached_snapshot(&self, chain_id: &str) -> Option<ClockSnapshot> {
        let mut snapshot = {
            let cache = self.cache.read().await;
            cache.get(chain_id).map(|entry| entry.snapshot().clone())?
        };
        self.annotate_snapshot(chain_id, &mut snapshot).await;
        apply_cached_validator_types_to_snapshot(self, chain_id, &mut snapshot).await;
        Some(snapshot)
    }

    pub(crate) async fn store_cached_snapshot(
        &self,
        chain_id: &str,
        fetched_at: u64,
        snapshot: ClockSnapshot,
    ) {
        self.cache
            .write()
            .await
            .insert(chain_id.to_owned(), CacheEntry::new(fetched_at, snapshot));
    }
}
