use crate::config::AppConfig;
use anyhow::{Context, Result};
use serde_json::Value;
use std::io::ErrorKind;

mod file_cache;
mod matching;
mod parsing;

use file_cache::load_map_nodes_file;
use parsing::ensure_map_nodes_array;

pub(crate) use matching::{filter_map_nodes_to_validators, map_nodes_by_peer};

pub(crate) const BUNDLED_TYCHO_MAP_CHAIN_ID: &str = "tycho-testnet";
pub(crate) const BUNDLED_TON_MAP_CHAIN_ID: &str = "ton";

const APP_TYCHO_NODES_JS: &str = include_str!("../../public/app/tycho_nodes.js");
const APP_TON_NODES_JSON: &str = include_str!("../../public/app/ton_nodes.json");

#[derive(Clone)]
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
