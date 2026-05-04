use crate::chain::CacheEntry;
use crate::config::AppConfig;
use crate::history::{RoundHistoryStore, load_round_history};
use crate::validator_types::{ValidatorTypeCache, load_validator_type_cache};
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use tracing::warn;

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
        Self {
            config: Arc::clone(&config),
            started_at: SystemTime::now(),
            cache: RwLock::new(HashMap::new()),
            chain_status: RwLock::new(HashMap::new()),
            validator_type_cache: RwLock::new(
                load_validator_type_cache(&validator_type_cache_path).unwrap_or_else(|error| {
                    warn!(path = %validator_type_cache_path.display(), error = ?error, "failed to load validator type cache");
                    ValidatorTypeCache::default()
                }),
            ),
            validator_type_cache_path,
            round_history: RwLock::new(
                load_round_history(&round_history_path).unwrap_or_else(|error| {
                    warn!(path = %round_history_path.display(), error = ?error, "failed to load round history");
                    RoundHistoryStore::default()
                }),
            ),
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
