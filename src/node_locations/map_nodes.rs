//! The map node files the frontend reads, built from candidates and locations.

use super::candidates::CandidateNode;
use super::fields::{is_zero, unknown_if_empty, unknown_string};
use super::geo_cache::{CachedGeoLocation, GeoCache};
use super::manual_review::ManualResolvedIp;
use crate::fsutil::write_file_atomic;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::net::IpAddr;
use std::path::Path;
use std::time::UNIX_EPOCH;

pub(super) const MAP_NODE_RETENTION_SECONDS: u64 = 60 * 60;

#[cfg(test)]
pub(super) fn build_map_nodes_from_candidates(
    candidates: &[CandidateNode],
    geo_cache: &GeoCache,
    manual_resolved: &BTreeMap<IpAddr, ManualResolvedIp>,
) -> Vec<MapNode> {
    build_map_nodes_from_candidates_with_retention(
        candidates,
        geo_cache,
        manual_resolved,
        &PreviousMapNodes::default(),
        0,
    )
    .nodes
}

pub(super) fn build_map_nodes_from_candidates_with_retention(
    candidates: &[CandidateNode],
    geo_cache: &GeoCache,
    manual_resolved: &BTreeMap<IpAddr, ManualResolvedIp>,
    previous_nodes: &PreviousMapNodes,
    now: u64,
) -> BuiltMapNodes {
    let mut nodes = Vec::new();
    let mut seen = BTreeSet::new();
    let mut current_peers = BTreeSet::new();
    let mut blocked_peers = BTreeSet::new();

    for candidate in candidates {
        let peer = candidate.peer_key();
        if let Some(manual) = manual_resolved.get(&candidate.ip) {
            if seen.insert(candidate.key()) {
                current_peers.insert(peer);
                nodes.push(MapNode::from_manual(candidate, manual, now));
            }
            continue;
        }
        let Some(location) = geo_cache.location(candidate.ip) else {
            continue;
        };
        if location.ipinfo_conflict {
            blocked_peers.insert(peer);
            continue;
        }
        if !location.has_coordinates() {
            continue;
        }
        if !seen.insert(candidate.key()) {
            continue;
        }
        current_peers.insert(peer);
        nodes.push(MapNode::from_cached_location(candidate, location, now));
    }

    let mut retained_node_count = 0;
    for previous in &previous_nodes.nodes {
        let peer = previous.peer_key();
        if peer.is_empty() || current_peers.contains(&peer) || blocked_peers.contains(&peer) {
            continue;
        }
        if !previous.is_retained(now, previous_nodes.updated_at) {
            continue;
        }
        if seen.insert(previous.key()) {
            nodes.push(previous.clone());
            retained_node_count += 1;
        }
    }

    nodes.sort_by(|left, right| {
        left.country
            .cmp(&right.country)
            .then_with(|| left.city.cmp(&right.city))
            .then_with(|| left.ip.cmp(&right.ip))
            .then_with(|| left.peer.cmp(&right.peer))
    });
    BuiltMapNodes {
        nodes,
        retained_node_count,
    }
}

pub(super) fn write_map_nodes_atomic(path: &Path, nodes: &[MapNode]) -> Result<()> {
    let data = serde_json::to_vec_pretty(nodes).context("failed to serialize map nodes")?;
    write_file_atomic(path, &data, 0o644)
}

pub(super) fn load_existing_map_nodes(path: &Path) -> Result<PreviousMapNodes> {
    if !path.exists() {
        return Ok(PreviousMapNodes::default());
    }

    let body =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let nodes = serde_json::from_str::<Vec<MapNode>>(&body)
        .with_context(|| format!("failed to parse {}", path.display()))?;
    let updated_at = fs::metadata(path)
        .ok()
        .and_then(|metadata| metadata.modified().ok())
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs());
    Ok(PreviousMapNodes { nodes, updated_at })
}

#[derive(Debug, Default)]
pub(super) struct BuiltMapNodes {
    pub(super) nodes: Vec<MapNode>,
    pub(super) retained_node_count: usize,
}

#[derive(Debug, Default)]
pub(super) struct PreviousMapNodes {
    pub(super) nodes: Vec<MapNode>,
    pub(super) updated_at: Option<u64>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct MapNode {
    pub(super) peer: String,
    pub(super) ip: String,
    pub(super) city: String,
    pub(super) country: String,
    pub(super) isp: String,
    pub(super) lat: f64,
    pub(super) lon: f64,
    pub(super) geo_source: String,
    pub(super) geo_confidence: String,
    pub(super) geo_updated_at: u64,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub(super) last_seen_at: u64,
}

impl MapNode {
    pub(super) fn from_cached_location(
        candidate: &CandidateNode,
        location: &CachedGeoLocation,
        now: u64,
    ) -> Self {
        Self {
            peer: candidate.peer.clone(),
            ip: candidate.ip.to_string(),
            city: location.city.clone(),
            country: location.country.clone(),
            isp: location.isp.clone(),
            lat: location.lat,
            lon: location.lon,
            geo_source: location.source.clone(),
            geo_confidence: location.confidence.clone(),
            geo_updated_at: location.updated_at,
            last_seen_at: now,
        }
    }

    pub(super) fn from_manual(
        candidate: &CandidateNode,
        manual: &ManualResolvedIp,
        now: u64,
    ) -> Self {
        Self {
            peer: candidate.peer.clone(),
            ip: candidate.ip.to_string(),
            city: unknown_if_empty(&manual.geo.city),
            country: unknown_if_empty(&manual.geo.country),
            isp: manual
                .as_info
                .as_ref()
                .map_or_else(unknown_string, |as_info| unknown_if_empty(&as_info.name)),
            lat: manual.geo.latitude,
            lon: manual.geo.longitude,
            geo_source: "manual".to_owned(),
            geo_confidence: "manual".to_owned(),
            geo_updated_at: manual.updated_at.unwrap_or_default(),
            last_seen_at: now,
        }
    }

    pub(super) fn key(&self) -> (String, String) {
        (self.peer_key(), self.ip.to_ascii_lowercase())
    }

    pub(super) fn peer_key(&self) -> String {
        self.peer.to_ascii_lowercase()
    }

    pub(super) fn is_retained(&self, now: u64, fallback_seen_at: Option<u64>) -> bool {
        let last_seen_at = if self.last_seen_at == 0 {
            fallback_seen_at.unwrap_or_default()
        } else {
            self.last_seen_at
        };
        last_seen_at != 0 && now.saturating_sub(last_seen_at) < MAP_NODE_RETENTION_SECONDS
    }
}
