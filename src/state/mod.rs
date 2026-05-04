mod acme;
mod cache;
mod history;
mod runtime;
mod validator_types;

use self::runtime::ChainRuntimeStatus;
use crate::chain::CacheEntry;
use crate::config::AppConfig;
use crate::history::RoundHistoryStore;
use crate::validator_types::ValidatorTypeCache;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use tracing::info;

pub(crate) struct AppState {
    pub(crate) config: Arc<AppConfig>,
    started_at: SystemTime,
    cache: RwLock<HashMap<String, CacheEntry>>,
    chain_status: RwLock<HashMap<String, ChainRuntimeStatus>>,
    validator_type_cache_path: PathBuf,
    validator_type_cache: RwLock<ValidatorTypeCache>,
    round_history_path: PathBuf,
    round_history: RwLock<RoundHistoryStore>,
    acme_challenges: RwLock<HashMap<String, String>>,
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

        history::log_chain_history_paths(&config, &round_history_path);
        let validator_type_cache = validator_types::load_initial_cache(&validator_type_cache_path);
        let round_history = history::load_initial_store(&config, &round_history_path);

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
}
