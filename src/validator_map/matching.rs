use super::parsing::{map_node_peer, map_nodes_array, string_field};
use crate::chain::{ValidatorDto, ValidatorMapNodeDto};
use anyhow::Result;
use serde_json::Value;
use std::collections::{HashMap, HashSet};

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
