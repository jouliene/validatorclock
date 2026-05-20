use crate::chain::ValidatorDto;
use crate::config::AppConfig;
use anyhow::{Context, Result, bail};
use serde_json::Value;
use std::collections::HashSet;
use std::io::ErrorKind;

pub(crate) const TYCHO_MAP_CHAIN_ID: &str = "tycho-testnet";

const APP_TYCHO_NODES_JS: &str = include_str!("../public/app/tycho_nodes.js");

pub(crate) fn load_map_nodes(config: &AppConfig, chain_id: &str) -> Result<Option<Value>> {
    if let Some(path) = config.map_nodes_paths.get(chain_id) {
        let body = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let value = serde_json::from_str(&body)
            .with_context(|| format!("failed to parse {}", path.display()))?;
        return ensure_map_nodes_array(value).map(Some);
    }

    if chain_id == TYCHO_MAP_CHAIN_ID {
        return load_tycho_map_nodes(config).map(Some);
    }

    Ok(None)
}

pub(crate) fn load_tycho_map_nodes(config: &AppConfig) -> Result<Value> {
    if let Some(path) = &config.tycho_map_nodes_path {
        match std::fs::read_to_string(path) {
            Ok(body) => {
                let value = serde_json::from_str(&body)
                    .with_context(|| format!("failed to parse {}", path.display()))?;
                return ensure_map_nodes_array(value);
            }
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error).with_context(|| format!("failed to read {}", path.display()));
            }
        }
    }

    let value = fallback_tycho_nodes_json().context("failed to parse bundled Tycho map nodes")?;
    ensure_map_nodes_array(value)
}

pub(crate) fn fallback_tycho_nodes_json() -> Result<Value, serde_json::Error> {
    let body = APP_TYCHO_NODES_JS
        .trim()
        .strip_prefix("window.TYCHO_NODES =")
        .and_then(|body| body.trim().strip_suffix(';'))
        .unwrap_or("[]");

    serde_json::from_str(body.trim())
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
