//! Node addresses seeded from the local resolver files.

use super::fields::string_field;
use crate::config::NodeLocationChainConfig;
use anyhow::{Context, Result, anyhow};
use serde_json::Value;
use std::collections::BTreeSet;
use std::net::{IpAddr, SocketAddr};

pub(super) fn collect_local_file_candidates(
    chain_config: &NodeLocationChainConfig,
) -> Result<Vec<CandidateNode>> {
    let input_path = chain_config
        .input_path
        .as_deref()
        .ok_or_else(|| anyhow!("input_path is required"))?;
    let body = std::fs::read_to_string(input_path)
        .with_context(|| format!("failed to read {}", input_path.display()))?;
    let value: Value = serde_json::from_str(&body)
        .with_context(|| format!("failed to parse {}", input_path.display()))?;
    let mut candidates = collect_candidates_from_value(&value, None);
    candidates = unique_candidates(candidates);
    Ok(candidates)
}

pub(super) fn collect_candidates_from_value(
    value: &Value,
    fallback_peer: Option<&str>,
) -> Vec<CandidateNode> {
    match value {
        Value::Array(items) => items
            .iter()
            .flat_map(|item| collect_candidates_from_value(item, fallback_peer))
            .collect(),
        Value::Object(object) => {
            for field in ["nodes", "peers", "validators", "data"] {
                if let Some(items) = object.get(field).and_then(Value::as_array) {
                    return items
                        .iter()
                        .flat_map(|item| collect_candidates_from_value(item, fallback_peer))
                        .collect();
                }
            }

            if looks_like_record(value) || fallback_peer.is_some() {
                return record_candidates(value, fallback_peer);
            }

            object
                .iter()
                .flat_map(|(peer, item)| collect_candidates_from_value(item, Some(peer)))
                .collect()
        }
        Value::String(_) => record_candidates(value, fallback_peer),
        _ => Vec::new(),
    }
}

pub(super) fn looks_like_record(value: &Value) -> bool {
    local_record_peer(value).is_some() || !local_record_ips(value).is_empty()
}

pub(super) fn record_candidates(record: &Value, fallback_peer: Option<&str>) -> Vec<CandidateNode> {
    let Some(peer) = local_record_peer(record).or_else(|| fallback_peer.map(str::to_owned)) else {
        return Vec::new();
    };
    let confirmed_at = local_record_confirmed_at(record);
    local_record_ips(record)
        .into_iter()
        .map(|ip| CandidateNode {
            peer: peer.clone(),
            ip,
            confirmed_at,
        })
        .collect()
}

/// When the resolver last actually reached this node, if it says.
///
/// The resolver keeps an address for an hour after the last time it answered,
/// so a record here may be a memory rather than a sighting. It says which, and
/// says when: without carrying that through, everything downstream reads the
/// file's own freshness as the node's, and a node nobody has reached for fifty
/// minutes is published as seen just now.
pub(super) fn local_record_confirmed_at(record: &Value) -> Option<u64> {
    fn from_object(value: &Value) -> Option<u64> {
        value.get("confirmed_at").and_then(Value::as_u64)
    }

    from_object(record).or_else(|| record.get("resolution").and_then(from_object))
}

pub(super) fn local_record_peer(record: &Value) -> Option<String> {
    [
        "peer",
        "peer_id",
        "public_key",
        "validator_public_key",
        "validator",
        "validator_id",
        "id",
    ]
    .into_iter()
    .find_map(|field| string_field(record, field))
}

pub(super) fn local_record_ips(record: &Value) -> Vec<IpAddr> {
    let mut ips = BTreeSet::new();
    collect_ips_from_value(record, &mut ips);
    ips.into_iter().collect()
}

pub(super) fn collect_ips_from_value(value: &Value, ips: &mut BTreeSet<IpAddr>) {
    match value {
        Value::String(address) => {
            if let Some(ip) = extract_ip_from_address(address) {
                ips.insert(ip);
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_ips_from_value(item, ips);
            }
        }
        Value::Object(object) => {
            for field in [
                "ip",
                "address",
                "addr",
                "endpoint",
                "host",
                "validator_ip",
                "addresses",
                "address_list",
                "ips",
                "info",
                "resolution",
            ] {
                if let Some(item) = object.get(field) {
                    collect_ips_from_value(item, ips);
                }
            }
        }
        _ => {}
    }
}

pub(super) fn extract_ip_from_address(address: &str) -> Option<IpAddr> {
    let trimmed = address.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Ok(socket_addr) = trimmed.parse::<SocketAddr>() {
        return Some(socket_addr.ip());
    }
    if let Ok(ip) = trimmed.parse::<IpAddr>() {
        return Some(ip);
    }
    if let Some(stripped) = trimmed.strip_prefix('[')
        && let Some((host, _rest)) = stripped.split_once(']')
    {
        return host.parse::<IpAddr>().ok();
    }
    if let Some((host, _port)) = trimmed.rsplit_once(':')
        && !host.contains(':')
    {
        return host.parse::<IpAddr>().ok();
    }
    None
}

pub(super) fn unique_candidates(candidates: Vec<CandidateNode>) -> Vec<CandidateNode> {
    let mut seen = BTreeSet::new();
    let mut unique = Vec::new();
    for candidate in candidates {
        let key = (
            candidate.peer.to_ascii_lowercase(),
            candidate.ip.to_string().to_ascii_lowercase(),
        );
        if seen.insert(key) {
            unique.push(candidate);
        }
    }
    unique.sort_by(|left, right| {
        left.peer
            .cmp(&right.peer)
            .then_with(|| left.ip.cmp(&right.ip))
    });
    unique
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct CandidateNode {
    pub(super) peer: String,
    pub(super) ip: IpAddr,
    /// When this address was last confirmed, when the source says. `None` for
    /// a source that does not date its records - then the reading itself is
    /// the only timestamp there is.
    pub(super) confirmed_at: Option<u64>,
}

impl CandidateNode {
    pub(super) fn key(&self) -> (String, String) {
        (self.peer_key(), self.ip.to_string().to_ascii_lowercase())
    }

    pub(super) fn peer_key(&self) -> String {
        self.peer.to_ascii_lowercase()
    }

    /// When this address was last known to be good.
    ///
    /// The reading time is the fallback, not the answer: it is right only for a
    /// source that has just confirmed the address, which is exactly the case a
    /// source that dates its records tells us about.
    pub(super) fn last_seen_at(&self, now: u64) -> u64 {
        self.confirmed_at.unwrap_or(now)
    }
}
