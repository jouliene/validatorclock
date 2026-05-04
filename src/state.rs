use crate::chain::{CacheEntry, ClockSnapshot};
use crate::config::AppConfig;
use crate::history::{
    RoundHistoryStore, load_round_history_for_chains, round_history_chain_path,
    save_round_history_merged,
};
use crate::validator_types::{ValidatorTypeCache, load_validator_type_cache};
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use tracing::{info, warn};

#[derive(Debug, Clone, Default, Serialize)]
pub(crate) struct ChainRuntimeStatus {
    pub(crate) last_attempt_at: Option<u64>,
    pub(crate) last_success_at: Option<u64>,
    pub(crate) last_error: Option<String>,
}

pub(crate) struct AppState {
    pub(crate) config: Arc<AppConfig>,
    pub(crate) started_at: SystemTime,
    cache: RwLock<HashMap<String, CacheEntry>>,
    chain_status: RwLock<HashMap<String, ChainRuntimeStatus>>,
    pub(crate) validator_type_cache_path: PathBuf,
    pub(crate) validator_type_cache: RwLock<ValidatorTypeCache>,
    round_history_path: PathBuf,
    round_history: RwLock<RoundHistoryStore>,
    pub(crate) acme_challenges: RwLock<HashMap<String, String>>,
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
    pub(crate) fn new(config: Arc<AppConfig>) -> Self {
        let round_history_path = config.effective_history_path();
        let validator_type_cache_path = config.effective_validator_type_cache_path();
        info!(
            cache_path = %config.cache_path.display(),
            history_base_path = %round_history_path.display(),
            validator_type_cache_path = %validator_type_cache_path.display(),
            chains = config.chains.len(),
            "runtime state paths configured"
        );
        for chain in &config.chains {
            info!(
                chain_id = %chain.id,
                history_path = %round_history_chain_path(&round_history_path, &chain.id).display(),
                "chain round history path configured"
            );
        }

        let validator_type_cache =
            load_validator_type_cache(&validator_type_cache_path).unwrap_or_else(|error| {
                warn!(path = %validator_type_cache_path.display(), error = ?error, "failed to load validator type cache");
                ValidatorTypeCache::default()
            });
        info!(
            path = %validator_type_cache_path.display(),
            entries = validator_type_cache.len(),
            exists = validator_type_cache_path.exists(),
            "loaded validator type cache"
        );

        let round_history = load_round_history_for_chains(
            &round_history_path,
            config.chains.iter().map(|chain| chain.id.as_str()),
        )
        .unwrap_or_else(|error| {
            warn!(path = %round_history_path.display(), error = ?error, "failed to load round history");
            RoundHistoryStore::default()
        });
        for chain in &config.chains {
            let history_path = round_history_chain_path(&round_history_path, &chain.id);
            info!(
                chain_id = %chain.id,
                path = %history_path.display(),
                exists = history_path.exists(),
                rounds = round_history.round_count_for_chain(&chain.id),
                "loaded chain round history"
            );
        }

        Self {
            config: Arc::clone(&config),
            started_at: SystemTime::now(),
            cache: RwLock::new(HashMap::new()),
            chain_status: RwLock::new(HashMap::new()),
            validator_type_cache: RwLock::new(validator_type_cache),
            validator_type_cache_path,
            round_history: RwLock::new(round_history),
            round_history_path,
            acme_challenges: RwLock::new(HashMap::new()),
        }
    }

    pub(crate) fn uptime_seconds(&self) -> u64 {
        self.started_at.elapsed().unwrap_or_default().as_secs()
    }

    pub(crate) fn started_at_seconds(&self) -> Option<u64> {
        self.started_at
            .duration_since(UNIX_EPOCH)
            .ok()
            .map(|duration| duration.as_secs())
    }

    fn round_history_path_for_chain(&self, chain_id: &str) -> PathBuf {
        round_history_chain_path(&self.round_history_path, chain_id)
    }

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
        Some(snapshot)
    }

    pub(crate) async fn cached_snapshot(&self, chain_id: &str) -> Option<ClockSnapshot> {
        let mut snapshot = {
            let cache = self.cache.read().await;
            cache.get(chain_id).map(|entry| entry.snapshot().clone())?
        };
        self.annotate_snapshot(chain_id, &mut snapshot).await;
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

    pub(crate) async fn record_round_history(
        &self,
        snapshot: &mut ClockSnapshot,
        observed_at: u64,
    ) {
        let chain_id = snapshot.chain_id().to_owned();
        let retention = RoundHistoryStore::retention_for_snapshot(&chain_id, snapshot);
        let history_path = self.round_history_path_for_chain(&chain_id);
        let history_to_save = {
            let mut history = self.round_history.write().await;
            let rounds_before = history.round_count_for_chain(&chain_id);
            let changed = history.record_snapshot(&chain_id, snapshot, observed_at);
            history.annotate_snapshot(&chain_id, snapshot);
            let rounds_after = history.round_count_for_chain(&chain_id);
            if changed || !history_path.exists() {
                info!(
                    chain_id,
                    path = %history_path.display(),
                    rounds_before,
                    rounds_after,
                    changed,
                    "round history scheduled for save"
                );
            }
            (changed || !history_path.exists()).then(|| history.clone())
        };

        let Some(history_to_save) = history_to_save else {
            return;
        };

        let history_base_path = self.round_history_path.clone();
        let log_history_path = history_path.clone();
        match tokio::task::spawn_blocking(move || {
            save_round_history_merged(&history_base_path, &chain_id, &history_to_save, &retention)
        })
        .await
        {
            Ok(Ok(saved_history)) => {
                self.round_history.write().await.merge_from(saved_history);
            }
            Ok(Err(error)) => {
                warn!(
                    path = %log_history_path.display(),
                    error = ?error,
                    "failed to save round history"
                );
            }
            Err(error) => {
                warn!(
                    path = %log_history_path.display(),
                    error = ?error,
                    "round history save task failed"
                );
            }
        }
    }

    async fn annotate_snapshot(&self, chain_id: &str, snapshot: &mut ClockSnapshot) {
        self.round_history
            .read()
            .await
            .annotate_snapshot(chain_id, snapshot);
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
}
