use crate::validator_types::{ValidatorTypeCache, load_validator_type_cache};
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
