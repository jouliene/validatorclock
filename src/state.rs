use crate::chain::CacheEntry;
use crate::config::AppConfig;
use crate::history::{RoundHistoryStore, load_round_history_for_chains, round_history_chain_path};
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
    pub(crate) cache: RwLock<HashMap<String, CacheEntry>>,
    pub(crate) chain_status: RwLock<HashMap<String, ChainRuntimeStatus>>,
    pub(crate) validator_type_cache_path: PathBuf,
    pub(crate) validator_type_cache: RwLock<ValidatorTypeCache>,
    pub(crate) round_history_path: PathBuf,
    pub(crate) round_history: RwLock<RoundHistoryStore>,
    pub(crate) acme_challenges: RwLock<HashMap<String, String>>,
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

    pub(crate) fn round_history_path_for_chain(&self, chain_id: &str) -> PathBuf {
        round_history_chain_path(&self.round_history_path, chain_id)
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
