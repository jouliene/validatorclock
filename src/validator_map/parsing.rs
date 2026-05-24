use super::MapNodesPayload;
use anyhow::{Context, Result};
use serde_json::Value;
use std::path::Path;

pub(super) fn parse_map_nodes_body(
    path: &Path,
    body: &str,
    updated_at: Option<u64>,
) -> Result<MapNodesPayload> {
    let value = serde_json::from_str(body)
        .with_context(|| format!("failed to parse {}", path.display()))?;
    let nodes = ensure_map_nodes_array(value)?;
    Ok(MapNodesPayload { nodes, updated_at })
}

pub(super) fn ensure_map_nodes_array(value: Value) -> Result<Value> {
    map_nodes_array(&value)?;
    Ok(value)
}

pub(super) fn map_nodes_array(value: &Value) -> Result<&[Value]> {
    value
        .as_array()
        .map(Vec::as_slice)
        .context("validator map nodes payload must be a JSON array")
}

pub(super) fn map_node_peer(node: &Value) -> Option<String> {
    string_field(node, "peer").map(|peer| peer.to_ascii_lowercase())
}

pub(super) fn string_field(node: &Value, field: &str) -> Option<String> {
    node.get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}
