use super::AppState;
use crate::chain::{CacheEntry, ClockSnapshot, apply_cached_validator_types_to_snapshot};
use crate::config::ChainConfig;
use crate::fsutil::write_file_atomic;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use tracing::{info, warn};

const SNAPSHOT_CACHE_VERSION: u32 = 1;

#[derive(Debug, Default, Deserialize, Serialize)]
struct SnapshotCacheDisk {
    version: u32,
    #[serde(default)]
    chains: HashMap<String, CacheEntry>,
}

pub(super) fn load_initial_cache(
    path: &Path,
    configured_chains: &[ChainConfig],
) -> HashMap<String, CacheEntry> {
    let mut cache = match load_snapshot_cache(path) {
        Ok(cache) => cache,
        Err(error) => {
            if path.exists() {
                warn!(path = %path.display(), error = ?error, "failed to load snapshot cache");
            }
            HashMap::new()
        }
    };

    let configured_chain_ids = configured_chains
        .iter()
        .map(|chain| chain.id.as_str())
        .collect::<HashSet<_>>();
    cache.retain(|chain_id, _| configured_chain_ids.contains(chain_id.as_str()));

    info!(
        path = %path.display(),
        entries = cache.len(),
        exists = path.exists(),
        "loaded snapshot cache"
    );
    cache
}

fn load_snapshot_cache(path: &Path) -> Result<HashMap<String, CacheEntry>> {
    let bytes = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    let disk: SnapshotCacheDisk = serde_json::from_slice(&bytes)
        .with_context(|| format!("failed to parse {}", path.display()))?;
    Ok(disk.chains)
}

fn save_snapshot_cache(path: &Path, cache: &HashMap<String, CacheEntry>) -> Result<()> {
    let disk = SnapshotCacheDisk {
        version: SNAPSHOT_CACHE_VERSION,
        chains: cache.clone(),
    };
    let data = serde_json::to_vec_pretty(&disk)
        .with_context(|| format!("failed to serialize {}", path.display()))?;
    write_file_atomic(path, &data, 0o600)
}

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
        self.annotate_map_fake_validators(&mut snapshot);
        if snapshot.current_set.fake_validator_status_known {
            self.record_round_history(&mut snapshot, now).await;
        }
        self.annotate_snapshot(chain_id, &mut snapshot).await;
        apply_cached_validator_types_to_snapshot(self, chain_id, &mut snapshot).await;
        Some(snapshot)
    }

    pub(crate) async fn cached_snapshot(&self, chain_id: &str) -> Option<ClockSnapshot> {
        let mut snapshot = {
            let cache = self.cache.read().await;
            cache.get(chain_id).map(|entry| entry.snapshot().clone())?
        };
        self.annotate_map_fake_validators(&mut snapshot);
        let observed_at = snapshot.fetched_at();
        if snapshot.current_set.fake_validator_status_known {
            self.record_round_history(&mut snapshot, observed_at).await;
        }
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
        {
            let mut cache = self.cache.write().await;
            cache.insert(chain_id.to_owned(), CacheEntry::new(fetched_at, snapshot));
        }
        self.save_snapshot_cache().await;
    }

    async fn save_snapshot_cache(&self) {
        let _guard = self.cache_save_lock.lock().await;
        let cache_to_save = self.cache.read().await.clone();
        let path = self.cache_path.clone();
        let log_path = path.clone();
        match tokio::task::spawn_blocking(move || save_snapshot_cache(&path, &cache_to_save)).await
        {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                warn!(
                    path = %log_path.display(),
                    error = ?error,
                    "failed to save snapshot cache"
                );
            }
            Err(error) => {
                warn!(
                    path = %log_path.display(),
                    error = ?error,
                    "snapshot cache save task failed"
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chain::test_clock_snapshot;
    use std::path::PathBuf;

    #[test]
    fn snapshot_cache_round_trips_and_filters_configured_chains() -> Result<()> {
        let path = temp_cache_path();
        let mut cache = HashMap::new();
        cache.insert(
            "test".to_owned(),
            CacheEntry::new(100, test_clock_snapshot("test")),
        );
        cache.insert(
            "removed".to_owned(),
            CacheEntry::new(200, test_clock_snapshot("removed")),
        );

        save_snapshot_cache(&path, &cache)?;
        let loaded = load_initial_cache(&path, &[test_chain("test")]);

        assert_eq!(loaded.len(), 1);
        assert_eq!(
            loaded
                .get("test")
                .map(|entry| entry.snapshot().current_set.round_id),
            Some(10)
        );
        assert!(!loaded.contains_key("removed"));

        let _ = fs::remove_file(path);
        Ok(())
    }

    fn test_chain(id: &str) -> ChainConfig {
        ChainConfig {
            id: id.to_owned(),
            name: id.to_owned(),
            rpc: "http://127.0.0.1".to_owned(),
            rpc_fallbacks: Vec::new(),
            color: "#38bdf8".to_owned(),
            token_symbol: "TEST".to_owned(),
            rpc_label: None,
        }
    }

    fn temp_cache_path() -> PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "validators_clock_snapshot_cache_test_{}_{}.json",
            std::process::id(),
            nonce
        ))
    }
}
