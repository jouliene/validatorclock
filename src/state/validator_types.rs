use super::AppState;
use crate::validator_types::{
    ValidatorTypeCache, load_validator_type_cache, save_validator_type_cache,
};
use std::path::Path;
use tracing::{info, warn};

pub(super) fn load_initial_cache(path: &Path) -> ValidatorTypeCache {
    let cache = load_validator_type_cache(path).unwrap_or_else(|error| {
        warn!(path = %path.display(), error = ?error, "failed to load validator type cache");
        ValidatorTypeCache::default()
    });
    info!(
        path = %path.display(),
        entries = cache.len(),
        exists = path.exists(),
        "loaded validator type cache"
    );
    cache
}

impl AppState {
    pub(crate) async fn with_validator_type_cache<R>(
        &self,
        read_cache: impl FnOnce(&ValidatorTypeCache) -> R,
    ) -> R {
        let cache = self.validator_type_cache.read().await;
        read_cache(&cache)
    }

    pub(crate) async fn update_validator_type_cache<R>(
        &self,
        update_cache: impl FnOnce(&mut ValidatorTypeCache) -> R,
    ) -> R {
        let mut cache = self.validator_type_cache.write().await;
        update_cache(&mut cache)
    }

    pub(crate) async fn save_validator_type_cache_background(
        &self,
        cache_to_save: ValidatorTypeCache,
    ) {
        let cache_path = self.validator_type_cache_path.clone();
        match tokio::task::spawn_blocking(move || {
            save_validator_type_cache(&cache_path, &cache_to_save)
        })
        .await
        {
            Ok(Ok(())) => {
                info!(
                    path = %self.validator_type_cache_path.display(),
                    "saved validator type cache"
                );
            }
            Ok(Err(error)) => {
                warn!(
                    path = %self.validator_type_cache_path.display(),
                    error = ?error,
                    "failed to save validator type cache"
                );
            }
            Err(error) => {
                warn!(
                    path = %self.validator_type_cache_path.display(),
                    error = ?error,
                    "validator type cache save task failed"
                );
            }
        }
    }
}
