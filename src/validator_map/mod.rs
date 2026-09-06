use crate::chain::ValidatorDto;
use crate::config::AppConfig;
use anyhow::Result;
use serde_json::Value;
use std::io::ErrorKind;

mod file_cache;
mod matching;
mod parsing;

use file_cache::load_map_nodes_file;

pub(crate) use matching::{filter_map_nodes_to_validators, map_nodes_by_peer};

#[derive(Clone)]
pub(crate) struct MapNodesPayload {
    pub(crate) nodes: Value,
    pub(crate) updated_at: Option<u64>,
}

/// Whether this chain has a map at all - a file to read, not a list of chain ids. The
/// page asks so it can offer the button, and asking this way means a chain given a map on
/// the server does not also need an edit to the page. The file is stat'ed rather than
/// read: the answer is whether there is one, not what is in it.
pub(crate) fn chain_has_map(config: &AppConfig, chain_id: &str) -> bool {
    let configured = config
        .map_nodes_paths
        .get(chain_id)
        .is_some_and(|path| path.exists());
    configured
        || config
            .node_location_output_path(chain_id)
            .is_some_and(|path| path.exists())
}

/// The map as a chain's readers see it: the nodes on file, kept to the
/// validators the chain has now.
pub(crate) fn active_map_nodes(
    config: &AppConfig,
    chain_id: &str,
    validators: &[ValidatorDto],
) -> Result<Option<Value>> {
    let Some(nodes) = load_map_nodes(config, chain_id)? else {
        return Ok(None);
    };
    filter_map_nodes_to_validators(nodes, validators).map(Some)
}

pub(crate) fn load_map_nodes(config: &AppConfig, chain_id: &str) -> Result<Option<Value>> {
    Ok(load_map_nodes_with_metadata(config, chain_id)?.map(|payload| payload.nodes))
}

pub(crate) fn load_map_nodes_with_metadata(
    config: &AppConfig,
    chain_id: &str,
) -> Result<Option<MapNodesPayload>> {
    if let Some(path) = config.map_nodes_paths.get(chain_id)
        && let Some(payload) = load_map_nodes_file_if_exists(path)?
    {
        return Ok(Some(payload));
    }

    if let Some(path) = config.node_location_output_path(chain_id)
        && let Some(payload) = load_map_nodes_file_if_exists(&path)?
    {
        return Ok(Some(payload));
    }

    // Nothing, and nothing is the answer.
    //
    // There used to be a snapshot of each map compiled into the binary, served
    // whenever the real one was missing. It did not read as a fallback from
    // outside: with the collector stopped and its file deleted on purpose, the
    // page went on showing 393 TON nodes from a picture taken four months
    // earlier, and neither the map nor /api/status said a word about it. A map
    // that cannot be drawn should look like a map that cannot be drawn.
    Ok(None)
}

fn load_map_nodes_file_if_exists(path: &std::path::Path) -> Result<Option<MapNodesPayload>> {
    match load_map_nodes_file(path) {
        Ok(payload) => Ok(Some(payload)),
        Err(error) if is_not_found_error(&error) => Ok(None),
        Err(error) => Err(error),
    }
}

fn is_not_found_error(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        cause
            .downcast_ref::<std::io::Error>()
            .is_some_and(|error| error.kind() == ErrorKind::NotFound)
    })
}
