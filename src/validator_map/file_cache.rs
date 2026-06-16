use super::MapNodesPayload;
use super::parsing::parse_map_nodes_body;
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, PartialEq, Eq)]
struct MapFileFingerprint {
    modified: Option<SystemTime>,
    len: u64,
}

impl MapFileFingerprint {
    fn updated_at(&self) -> Option<u64> {
        self.modified?
            .duration_since(UNIX_EPOCH)
            .ok()
            .map(|duration| duration.as_secs())
    }
}

struct CachedMapNodesFile {
    fingerprint: MapFileFingerprint,
    payload: MapNodesPayload,
}

pub(super) fn load_map_nodes_file(path: &Path) -> Result<MapNodesPayload> {
    load_map_nodes_file_with(path, read_map_file_fingerprint, |path| {
        std::fs::read_to_string(path)
    })
}

fn load_map_nodes_file_with(
    path: &Path,
    mut fingerprint_reader: impl FnMut(&Path) -> std::io::Result<MapFileFingerprint>,
    mut body_reader: impl FnMut(&Path) -> std::io::Result<String>,
) -> Result<MapNodesPayload> {
    let fingerprint = fingerprint_reader(path)
        .with_context(|| format!("failed to read metadata for {}", path.display()))?;
    let cache_key = map_file_cache_key(path);
    if let Some(payload) = cached_map_nodes_file(&cache_key, &fingerprint) {
        return Ok(payload);
    }

    let body = body_reader(path).with_context(|| format!("failed to read {}", path.display()))?;
    let payload = parse_map_nodes_body(path, &body, fingerprint.updated_at())?;
    cache_map_nodes_file(cache_key, fingerprint, payload.clone());
    Ok(payload)
}

fn read_map_file_fingerprint(path: &Path) -> std::io::Result<MapFileFingerprint> {
    let metadata = std::fs::metadata(path)?;
    Ok(MapFileFingerprint {
        modified: metadata.modified().ok(),
        len: metadata.len(),
    })
}

fn map_file_cache_key(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn map_nodes_file_cache() -> &'static Mutex<HashMap<PathBuf, CachedMapNodesFile>> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, CachedMapNodesFile>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn cached_map_nodes_file(
    cache_key: &Path,
    fingerprint: &MapFileFingerprint,
) -> Option<MapNodesPayload> {
    map_nodes_file_cache()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(cache_key)
        .filter(|entry| entry.fingerprint == *fingerprint)
        .map(|entry| entry.payload.clone())
}

fn cache_map_nodes_file(
    cache_key: PathBuf,
    fingerprint: MapFileFingerprint,
    payload: MapNodesPayload,
) {
    map_nodes_file_cache()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert(
            cache_key,
            CachedMapNodesFile {
                fingerprint,
                payload,
            },
        );
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::time::Duration;

    #[test]
    fn file_loader_reuses_cached_payload_when_metadata_is_unchanged() -> Result<()> {
        let _guard = test_cache_guard();
        clear_map_nodes_file_cache();
        let path = unique_test_path("cache-hit");
        let fingerprint = test_fingerprint(1, 42);
        let reads = Cell::new(0);

        let first = load_map_nodes_file_with(
            &path,
            |_| Ok(fingerprint.clone()),
            |_| {
                reads.set(reads.get() + 1);
                Ok(r#"[{"peer":"first"}]"#.to_owned())
            },
        )?;
        let second = load_map_nodes_file_with(
            &path,
            |_| Ok(fingerprint.clone()),
            |_| {
                reads.set(reads.get() + 1);
                Ok(r#"[{"peer":"second"}]"#.to_owned())
            },
        )?;

        assert_eq!(reads.get(), 1);
        assert_eq!(first.updated_at, Some(1));
        assert_eq!(first_peer(&first), Some("first"));
        assert_eq!(first_peer(&second), Some("first"));
        Ok(())
    }

    #[test]
    fn file_loader_reloads_payload_when_metadata_changes() -> Result<()> {
        let _guard = test_cache_guard();
        clear_map_nodes_file_cache();
        let path = unique_test_path("cache-reload");
        let modified_at = Cell::new(1);
        let reads = Cell::new(0);

        let first = load_map_nodes_file_with(
            &path,
            |_| Ok(test_fingerprint(modified_at.get(), 42)),
            |_| {
                reads.set(reads.get() + 1);
                Ok(r#"[{"peer":"first"}]"#.to_owned())
            },
        )?;

        modified_at.set(2);
        let second = load_map_nodes_file_with(
            &path,
            |_| Ok(test_fingerprint(modified_at.get(), 42)),
            |_| {
                reads.set(reads.get() + 1);
                Ok(r#"[{"peer":"second"}]"#.to_owned())
            },
        )?;

        assert_eq!(reads.get(), 2);
        assert_eq!(first.updated_at, Some(1));
        assert_eq!(second.updated_at, Some(2));
        assert_eq!(first_peer(&first), Some("first"));
        assert_eq!(first_peer(&second), Some("second"));
        Ok(())
    }

    fn test_cache_guard() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn clear_map_nodes_file_cache() {
        map_nodes_file_cache()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clear();
    }

    fn test_fingerprint(updated_at: u64, len: u64) -> MapFileFingerprint {
        MapFileFingerprint {
            modified: Some(UNIX_EPOCH + Duration::from_secs(updated_at)),
            len,
        }
    }

    fn first_peer(payload: &MapNodesPayload) -> Option<&str> {
        payload.nodes.as_array()?.first()?.get("peer")?.as_str()
    }

    fn unique_test_path(label: &str) -> PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "validatorclock_map_cache_{label}_{}_{}.json",
            std::process::id(),
            nonce
        ))
    }
}
