use crate::chain::{ValidatorDto, ValidatorMapNodeDto};
use crate::config::AppConfig;
use anyhow::{Context, Result};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) const BUNDLED_TYCHO_MAP_CHAIN_ID: &str = "tycho-testnet";
pub(crate) const BUNDLED_TON_MAP_CHAIN_ID: &str = "ton";

const APP_TYCHO_NODES_JS: &str = include_str!("../public/app/tycho_nodes.js");
const APP_TON_NODES_JSON: &str = include_str!("../public/app/ton_nodes.json");

#[derive(Clone)]
pub(crate) struct MapNodesPayload {
    pub(crate) nodes: Value,
    pub(crate) updated_at: Option<u64>,
}

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

pub(crate) fn load_map_nodes(config: &AppConfig, chain_id: &str) -> Result<Option<Value>> {
    Ok(load_map_nodes_with_metadata(config, chain_id)?.map(|payload| payload.nodes))
}

pub(crate) fn load_map_nodes_with_metadata(
    config: &AppConfig,
    chain_id: &str,
) -> Result<Option<MapNodesPayload>> {
    if let Some(path) = config.map_nodes_paths.get(chain_id) {
        return load_map_nodes_file(path).map(Some);
    }

    if chain_id == BUNDLED_TYCHO_MAP_CHAIN_ID {
        return load_tycho_map_nodes_with_metadata(config).map(Some);
    }

    if chain_id == BUNDLED_TON_MAP_CHAIN_ID {
        let value = fallback_ton_nodes_json().context("failed to parse bundled TON map nodes")?;
        let nodes = ensure_map_nodes_array(value)?;
        return Ok(Some(MapNodesPayload {
            nodes,
            updated_at: None,
        }));
    }

    Ok(None)
}

fn load_tycho_map_nodes_with_metadata(config: &AppConfig) -> Result<MapNodesPayload> {
    if let Some(path) = &config.tycho_map_nodes_path {
        match load_map_nodes_file(path) {
            Ok(payload) => return Ok(payload),
            Err(error) if is_not_found_error(&error) => {}
            Err(error) => return Err(error),
        }
    }

    let value = fallback_tycho_nodes_json().context("failed to parse bundled Tycho map nodes")?;
    let nodes = ensure_map_nodes_array(value)?;
    Ok(MapNodesPayload {
        nodes,
        updated_at: None,
    })
}

fn load_map_nodes_file(path: &Path) -> Result<MapNodesPayload> {
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

fn parse_map_nodes_body(
    path: &Path,
    body: &str,
    updated_at: Option<u64>,
) -> Result<MapNodesPayload> {
    let value = serde_json::from_str(body)
        .with_context(|| format!("failed to parse {}", path.display()))?;
    let nodes = ensure_map_nodes_array(value)?;
    Ok(MapNodesPayload { nodes, updated_at })
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

fn is_not_found_error(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        cause
            .downcast_ref::<std::io::Error>()
            .is_some_and(|error| error.kind() == ErrorKind::NotFound)
    })
}

pub(crate) fn fallback_tycho_nodes_json() -> Result<Value, serde_json::Error> {
    let body = APP_TYCHO_NODES_JS
        .trim()
        .strip_prefix("window.TYCHO_NODES =")
        .and_then(|body| body.trim().strip_suffix(';'))
        .unwrap_or("[]");

    serde_json::from_str(body.trim())
}

pub(crate) fn fallback_ton_nodes_json() -> Result<Value, serde_json::Error> {
    serde_json::from_str(APP_TON_NODES_JSON.trim())
}

pub(crate) fn filter_map_nodes_to_validators(
    value: Value,
    validators: &[ValidatorDto],
) -> Result<Value> {
    let active_peers = validators
        .iter()
        .map(|validator| validator.public_key.to_ascii_lowercase())
        .collect::<HashSet<_>>();

    let nodes = map_nodes_array(&value)?
        .iter()
        .filter(|node| {
            map_node_peer(node)
                .map(|peer| active_peers.contains(&peer))
                .unwrap_or_default()
        })
        .cloned()
        .collect::<Vec<_>>();

    Ok(Value::Array(nodes))
}

pub(crate) fn map_nodes_by_peer(value: &Value) -> Result<HashMap<String, ValidatorMapNodeDto>> {
    Ok(map_nodes_array(value)?
        .iter()
        .filter_map(|node| {
            map_node_peer(node).map(|peer| {
                (
                    peer,
                    ValidatorMapNodeDto {
                        ip: string_field(node, "ip"),
                        isp: string_field(node, "isp"),
                        city: string_field(node, "city"),
                        country: string_field(node, "country"),
                    },
                )
            })
        })
        .collect())
}

fn ensure_map_nodes_array(value: Value) -> Result<Value> {
    map_nodes_array(&value)?;
    Ok(value)
}

fn map_nodes_array(value: &Value) -> Result<&[Value]> {
    value
        .as_array()
        .map(Vec::as_slice)
        .context("validator map nodes payload must be a JSON array")
}

fn map_node_peer(node: &Value) -> Option<String> {
    string_field(node, "peer").map(|peer| peer.to_ascii_lowercase())
}

fn string_field(node: &Value, field: &str) -> Option<String> {
    node.get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::path::PathBuf;
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
            "validators_clock_map_cache_{label}_{}_{}.json",
            std::process::id(),
            nonce
        ))
    }
}
