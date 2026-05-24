use crate::chain::ValidatorDto;
use crate::config::AppConfig;
use anyhow::{Context, Result, bail};
use serde_json::Value;
use std::collections::HashSet;
use std::io::ErrorKind;
use std::path::Path;
use std::time::UNIX_EPOCH;

pub(crate) const TYCHO_MAP_CHAIN_ID: &str = "tycho-testnet";
pub(crate) const TON_MAP_CHAIN_ID: &str = "ton";

const APP_TYCHO_NODES_JS: &str = include_str!("../public/app/tycho_nodes.js");
const APP_TON_NODES_JSON: &str = include_str!("../public/app/ton_nodes.json");

pub(crate) struct MapNodesPayload {
    pub(crate) nodes: Value,
    pub(crate) updated_at: Option<u64>,
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

    if chain_id == TYCHO_MAP_CHAIN_ID {
        return load_tycho_map_nodes_with_metadata(config).map(Some);
    }

    if chain_id == TON_MAP_CHAIN_ID {
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
        match std::fs::read_to_string(path) {
            Ok(body) => {
                return parse_map_nodes_file(path, &body);
            }
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error).with_context(|| format!("failed to read {}", path.display()));
            }
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
    let body = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    parse_map_nodes_file(path, &body)
}

fn parse_map_nodes_file(path: &Path, body: &str) -> Result<MapNodesPayload> {
    let value = serde_json::from_str(body)
        .with_context(|| format!("failed to parse {}", path.display()))?;
    let nodes = ensure_map_nodes_array(value)?;
    Ok(MapNodesPayload {
        nodes,
        updated_at: file_modified_at(path),
    })
}

fn file_modified_at(path: &Path) -> Option<u64> {
    std::fs::metadata(path)
        .ok()?
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_secs())
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

    let nodes = value
        .as_array()
        .context("Tycho map nodes payload must be a JSON array")?
        .iter()
        .filter(|node| {
            node.get("peer")
                .and_then(Value::as_str)
                .map(|peer| active_peers.contains(&peer.to_ascii_lowercase()))
                .unwrap_or(false)
        })
        .cloned()
        .collect::<Vec<_>>();

    Ok(Value::Array(nodes))
}

pub(crate) fn mapped_peer_set(value: &Value) -> Result<HashSet<String>> {
    Ok(value
        .as_array()
        .context("Tycho map nodes payload must be a JSON array")?
        .iter()
        .filter_map(|node| node.get("peer").and_then(Value::as_str))
        .map(str::to_ascii_lowercase)
        .filter(|peer| !peer.is_empty())
        .collect())
}

fn ensure_map_nodes_array(value: Value) -> Result<Value> {
    if !value.is_array() {
        bail!("Tycho map nodes payload must be a JSON array");
    }
    Ok(value)
}
