use super::AppState;
use crate::chain::{CacheEntry, ClockSnapshot, apply_cached_validator_types_to_snapshot};
use crate::config::ChainConfig;
use crate::fsutil::{chain_file_path, write_file_atomic};
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
    let mut cache = HashMap::new();

    for chain in configured_chains {
        let chain_path = chain_file_path(path, &chain.id);
        match load_snapshot_cache(&chain_path) {
            Ok(entries) => cache.extend(
                entries
                    .into_iter()
                    .filter(|(chain_id, _)| chain_id == &chain.id),
            ),
            Err(error) => {
                if chain_path.exists() {
                    warn!(
                        path = %chain_path.display(),
                        error = ?error,
                        "failed to load chain snapshot cache"
                    );
                }
            }
        }
    }

    if cache.is_empty() {
        cache = migrate_combined_cache(path, configured_chains);
    }

    info!(
        path = %path.display(),
        entries = cache.len(),
        "loaded snapshot cache"
    );
    cache
}

/// Earlier releases kept every chain in one file. Split it once, then drop it:
/// the cache only shortens the first refresh after a restart.
fn migrate_combined_cache(
    path: &Path,
    configured_chains: &[ChainConfig],
) -> HashMap<String, CacheEntry> {
    if !path.exists() {
        return HashMap::new();
    }

    let mut cache = match load_snapshot_cache(path) {
        Ok(cache) => cache,
        Err(error) => {
            warn!(path = %path.display(), error = ?error, "failed to load snapshot cache");
            return HashMap::new();
        }
    };

    let configured_chain_ids = configured_chains
        .iter()
        .map(|chain| chain.id.as_str())
        .collect::<HashSet<_>>();
    cache.retain(|chain_id, _| configured_chain_ids.contains(chain_id.as_str()));

    for (chain_id, entry) in &cache {
        if let Err(error) = save_chain_cache(path, chain_id, entry) {
            warn!(
                chain_id,
                error = ?error,
                "failed to split the combined snapshot cache; keeping it as is"
            );
            return cache;
        }
    }

    match std::fs::remove_file(path) {
        Ok(()) => info!(
            path = %path.display(),
            chains = cache.len(),
            "split the combined snapshot cache into one file per chain"
        ),
        Err(error) => warn!(
            path = %path.display(),
            error = ?error,
            "failed to remove the combined snapshot cache after splitting it"
        ),
    }
    cache
}

fn load_snapshot_cache(path: &Path) -> Result<HashMap<String, CacheEntry>> {
    let bytes = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    let disk: SnapshotCacheDisk = serde_json::from_slice(&bytes)
        .with_context(|| format!("failed to parse {}", path.display()))?;
    Ok(disk.chains)
}

fn save_chain_cache(base_path: &Path, chain_id: &str, entry: &CacheEntry) -> Result<()> {
    let path = chain_file_path(base_path, chain_id);
    let disk = SnapshotCacheDisk {
        version: SNAPSHOT_CACHE_VERSION,
        chains: HashMap::from([(chain_id.to_owned(), entry.clone())]),
    };
    let data = serde_json::to_vec(&disk)
        .with_context(|| format!("failed to serialize {}", path.display()))?;
    write_file_atomic(&path, &data, 0o600)
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
        self.annotate_map_fake_validators(&mut snapshot, now).await;
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
        let observed_at = now_sec().unwrap_or_else(|| snapshot.fetched_at());
        self.annotate_map_fake_validators(&mut snapshot, observed_at)
            .await;
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
        let entry = {
            let mut cache = self.cache.write().await;
            let entry = CacheEntry::new(fetched_at, snapshot);
            cache.insert(chain_id.to_owned(), entry.clone());
            entry
        };
        self.save_chain_cache(chain_id, entry).await;
    }

    /// Only the refreshed chain is written; the other chains keep their files.
    async fn save_chain_cache(&self, chain_id: &str, entry: CacheEntry) {
        let _guard = self.cache_save_lock.lock().await;
        let base_path = self.cache_path.clone();
        let log_path = chain_file_path(&base_path, chain_id);
        let chain_id = chain_id.to_owned();
        match tokio::task::spawn_blocking(move || save_chain_cache(&base_path, &chain_id, &entry))
            .await
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

fn now_sec() -> Option<u64> {
    crate::timeutil::now_sec_checked().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chain::test_clock_snapshot;
    use std::path::PathBuf;

    #[test]
    fn each_chain_round_trips_through_its_own_file() -> Result<()> {
        let path = temp_cache_path();
        save_chain_cache(
            &path,
            "test",
            &CacheEntry::new(100, test_clock_snapshot("test")),
        )?;
        save_chain_cache(
            &path,
            "removed",
            &CacheEntry::new(200, test_clock_snapshot("removed")),
        )?;

        let loaded = load_initial_cache(&path, &[test_chain("test")]);

        assert_eq!(loaded.len(), 1, "only configured chains are loaded");
        assert_eq!(
            loaded
                .get("test")
                .map(|entry| entry.snapshot().current_set.round_id),
            Some(10)
        );
        assert!(!path.exists(), "the combined file is not written any more");

        for chain_id in ["test", "removed"] {
            let _ = fs::remove_file(chain_file_path(&path, chain_id));
        }
        Ok(())
    }

    #[test]
    fn a_combined_cache_from_an_earlier_release_is_split_and_removed() -> Result<()> {
        let path = temp_cache_path();
        let disk = SnapshotCacheDisk {
            version: SNAPSHOT_CACHE_VERSION,
            chains: HashMap::from([
                (
                    "test".to_owned(),
                    CacheEntry::new(100, test_clock_snapshot("test")),
                ),
                (
                    "removed".to_owned(),
                    CacheEntry::new(200, test_clock_snapshot("removed")),
                ),
            ]),
        };
        write_file_atomic(&path, &serde_json::to_vec(&disk)?, 0o600)?;

        let loaded = load_initial_cache(&path, &[test_chain("test")]);

        assert_eq!(loaded.len(), 1);
        assert!(
            !path.exists(),
            "the combined file is removed after splitting"
        );
        assert!(
            chain_file_path(&path, "test").exists(),
            "the configured chain gets its own file"
        );
        assert!(
            !chain_file_path(&path, "removed").exists(),
            "a chain that left the config is not carried over"
        );

        let reloaded = load_initial_cache(&path, &[test_chain("test")]);
        assert_eq!(reloaded.len(), 1);

        let _ = fs::remove_file(chain_file_path(&path, "test"));
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
            "validatorclock_snapshot_cache_test_{}_{}.json",
            std::process::id(),
            nonce
        ))
    }
}
