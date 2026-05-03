use crate::chain::{CacheEntry, ValidatorRoundCache, load_validator_round_disk_cache};
use crate::config::AppConfig;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::warn;

pub(crate) struct AppState {
    pub(crate) config: Arc<AppConfig>,
    pub(crate) cache: RwLock<HashMap<String, CacheEntry>>,
    pub(crate) validator_round_cache_path: PathBuf,
    pub(crate) validator_round_cache: ValidatorRoundCache,
    pub(crate) acme_challenges: RwLock<HashMap<String, String>>,
}

impl AppState {
    pub(crate) fn new(config: Arc<AppConfig>) -> Self {
        Self {
            config: Arc::clone(&config),
            cache: RwLock::new(HashMap::new()),
            validator_round_cache_path: config.cache_path.clone(),
            validator_round_cache: RwLock::new(
                load_validator_round_disk_cache(&config.cache_path).unwrap_or_else(|error| {
                    warn!(error = ?error, "failed to load validator round cache");
                    HashMap::new()
                }),
            ),
            acme_challenges: RwLock::new(HashMap::new()),
        }
    }
}
