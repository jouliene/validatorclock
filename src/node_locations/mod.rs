use crate::config::NodeLocationChainConfig;
use crate::fsutil::write_file_atomic;
use crate::state::AppState;
use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::net::{IpAddr, SocketAddr};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::time::sleep;
use tracing::{info, warn};

const IP_API_BATCH_SIZE: usize = 100;

pub(crate) fn spawn_background_refresh(state: Arc<AppState>) {
    if !state.config.node_locations.enabled {
        return;
    }

    tokio::spawn(async move {
        background_refresh_loop(state).await;
    });
}

async fn background_refresh_loop(state: Arc<AppState>) {
    let startup_delay = Duration::from_secs(state.config.node_locations.startup_delay_seconds);
    let refresh_seconds = state.config.node_locations.refresh_seconds.max(1);
    info!(
        refresh_seconds,
        startup_delay_seconds = startup_delay.as_secs(),
        "node location background refresh started"
    );

    if !startup_delay.is_zero() {
        sleep(startup_delay).await;
    }

    refresh_all_chains(Arc::clone(&state)).await;

    loop {
        sleep(Duration::from_secs(refresh_seconds)).await;
        refresh_all_chains(Arc::clone(&state)).await;
    }
}

async fn refresh_all_chains(state: Arc<AppState>) {
    let http = match reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
    {
        Ok(http) => http,
        Err(error) => {
            warn!(error = ?error, "failed to build node location HTTP client");
            return;
        }
    };
    let now = now_sec();
    let ttl = Duration::from_secs(state.config.node_locations.geo_cache_ttl_seconds);
    let mut geo_cache = match load_geo_cache(&state.config.node_locations.geo_cache_path) {
        Ok(cache) => cache,
        Err(error) => {
            warn!(
                path = %state.config.node_locations.geo_cache_path.display(),
                error = ?error,
                "failed to load node location geo cache"
            );
            GeoCache::default()
        }
    };
    let mut cache_changed = false;

    for chain in &state.config.chains {
        let chain_config = state.config.effective_node_location_chain(&chain.id);
        if !chain_config.enabled {
            continue;
        }

        match refresh_chain_locations(
            &http,
            &state.config.node_locations.ip_api_batch_endpoint,
            &chain.id,
            &chain_config,
            &mut geo_cache,
            now,
            ttl,
        )
        .await
        {
            Ok(changed) => {
                cache_changed |= changed;
            }
            Err(error) => {
                warn!(
                    chain_id = %chain.id,
                    error = ?error,
                    "node location refresh failed"
                );
            }
        }
    }

    if cache_changed
        && let Err(error) = save_geo_cache(&state.config.node_locations.geo_cache_path, &geo_cache)
    {
        warn!(
            path = %state.config.node_locations.geo_cache_path.display(),
            error = ?error,
            "failed to save node location geo cache"
        );
    }
}

async fn refresh_chain_locations(
    http: &reqwest::Client,
    ip_api_endpoint: &str,
    chain_id: &str,
    chain_config: &NodeLocationChainConfig,
    geo_cache: &mut GeoCache,
    now: u64,
    ttl: Duration,
) -> Result<bool> {
    let candidates = collect_local_file_candidates(chain_config)
        .with_context(|| format!("failed to collect node IP seeds for {chain_id}"))?;
    let ips = candidates
        .iter()
        .map(|candidate| candidate.ip)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let lookup_ips = ips
        .iter()
        .copied()
        .filter(|ip| !geo_cache.has_fresh_location(*ip, now, ttl))
        .collect::<Vec<_>>();

    let fetched = lookup_ip_api_locations(http, ip_api_endpoint, &lookup_ips, now).await;
    let mut cache_changed = false;
    for (ip, location) in fetched {
        geo_cache.locations.insert(ip.to_string(), location);
        cache_changed = true;
    }

    let nodes = build_map_nodes_from_candidates(&candidates, geo_cache);
    write_map_nodes_atomic(&chain_config.output_path, &nodes)?;

    info!(
        chain_id,
        seed_node_count = candidates.len(),
        unique_ip_count = ips.len(),
        provider_lookup_count = lookup_ips.len(),
        mapped_node_count = nodes.len(),
        output_path = %chain_config.output_path.display(),
        "published node location map"
    );

    Ok(cache_changed)
}

fn collect_local_file_candidates(
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

fn collect_candidates_from_value(value: &Value, fallback_peer: Option<&str>) -> Vec<CandidateNode> {
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

fn looks_like_record(value: &Value) -> bool {
    local_record_peer(value).is_some() || !local_record_ips(value).is_empty()
}

fn record_candidates(record: &Value, fallback_peer: Option<&str>) -> Vec<CandidateNode> {
    let Some(peer) = local_record_peer(record).or_else(|| fallback_peer.map(str::to_owned)) else {
        return Vec::new();
    };
    local_record_ips(record)
        .into_iter()
        .map(|ip| CandidateNode {
            peer: peer.clone(),
            ip,
        })
        .collect()
}

fn local_record_peer(record: &Value) -> Option<String> {
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

fn local_record_ips(record: &Value) -> Vec<IpAddr> {
    let mut ips = BTreeSet::new();
    collect_ips_from_value(record, &mut ips);
    ips.into_iter().collect()
}

fn collect_ips_from_value(value: &Value, ips: &mut BTreeSet<IpAddr>) {
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
            ] {
                if let Some(item) = object.get(field) {
                    collect_ips_from_value(item, ips);
                }
            }
        }
        _ => {}
    }
}

fn extract_ip_from_address(address: &str) -> Option<IpAddr> {
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

fn unique_candidates(candidates: Vec<CandidateNode>) -> Vec<CandidateNode> {
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

async fn lookup_ip_api_locations(
    http: &reqwest::Client,
    endpoint: &str,
    ips: &[IpAddr],
    now: u64,
) -> BTreeMap<IpAddr, CachedGeoLocation> {
    let mut output = BTreeMap::new();
    for chunk in ips.chunks(IP_API_BATCH_SIZE) {
        let requests = chunk.iter().map(IpAddr::to_string).collect::<Vec<_>>();
        let response = match http.post(endpoint).json(&requests).send().await {
            Ok(response) => response,
            Err(error) => {
                warn!(error = ?error, "ip-api batch lookup failed");
                continue;
            }
        };
        if !response.status().is_success() {
            warn!(status = %response.status(), "ip-api batch lookup returned an error");
            continue;
        }
        let raw = match response.json::<Vec<IpApiResponse>>().await {
            Ok(raw) => raw,
            Err(error) => {
                warn!(error = ?error, "failed to decode ip-api batch response");
                continue;
            }
        };
        output.extend(raw.into_iter().filter_map(|item| item.into_location(now)));
    }
    output
}

fn build_map_nodes_from_candidates(
    candidates: &[CandidateNode],
    geo_cache: &GeoCache,
) -> Vec<MapNode> {
    let mut nodes = Vec::new();
    let mut seen = BTreeSet::new();

    for candidate in candidates {
        let Some(location) = geo_cache.location(candidate.ip) else {
            continue;
        };
        if !location.has_coordinates() {
            continue;
        }
        let key = (
            candidate.peer.to_ascii_lowercase(),
            candidate.ip.to_string().to_ascii_lowercase(),
        );
        if !seen.insert(key) {
            continue;
        }
        nodes.push(MapNode {
            peer: candidate.peer.clone(),
            ip: candidate.ip.to_string(),
            city: unknown_if_empty(&location.city),
            country: unknown_if_empty(&location.country),
            isp: unknown_if_empty(&location.isp),
            lat: location.lat,
            lon: location.lon,
            geo_source: location.source.clone(),
            geo_confidence: location.confidence.clone(),
            geo_updated_at: location.updated_at,
        });
    }

    nodes.sort_by(|left, right| {
        left.country
            .cmp(&right.country)
            .then_with(|| left.city.cmp(&right.city))
            .then_with(|| left.ip.cmp(&right.ip))
            .then_with(|| left.peer.cmp(&right.peer))
    });
    nodes
}

fn write_map_nodes_atomic(path: &Path, nodes: &[MapNode]) -> Result<()> {
    let data = serde_json::to_vec_pretty(nodes).context("failed to serialize map nodes")?;
    write_file_atomic(path, &data, 0o644)
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CandidateNode {
    peer: String,
    ip: IpAddr,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct GeoCache {
    #[serde(flatten)]
    locations: BTreeMap<String, CachedGeoLocation>,
}

impl GeoCache {
    fn location(&self, ip: IpAddr) -> Option<&CachedGeoLocation> {
        self.locations.get(&ip.to_string())
    }

    fn has_fresh_location(&self, ip: IpAddr, now: u64, ttl: Duration) -> bool {
        self.location(ip)
            .is_some_and(|location| location.has_coordinates() && location.is_fresh(now, ttl))
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct CachedGeoLocation {
    #[serde(default = "unknown_string")]
    city: String,
    #[serde(default = "unknown_string")]
    country: String,
    #[serde(default = "unknown_string")]
    isp: String,
    lat: f64,
    lon: f64,
    #[serde(default = "ip_api_source")]
    source: String,
    #[serde(default = "medium_confidence")]
    confidence: String,
    #[serde(default)]
    updated_at: u64,
}

impl CachedGeoLocation {
    fn has_coordinates(&self) -> bool {
        self.lat.is_finite() && self.lon.is_finite()
    }

    fn is_fresh(&self, now: u64, ttl: Duration) -> bool {
        now.saturating_sub(self.updated_at) < ttl.as_secs()
    }
}

fn load_geo_cache(path: &Path) -> Result<GeoCache> {
    if !path.exists() {
        return Ok(GeoCache::default());
    }

    let body = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;
    let value: Value = serde_json::from_str(&body)
        .with_context(|| format!("failed to parse {}", path.display()))?;

    if value.get("version").is_some() && value.get("ips").is_some() {
        return Ok(migrate_versioned_geo_cache(value));
    }

    let locations = serde_json::from_value::<BTreeMap<String, CachedGeoLocation>>(value)
        .context("failed to parse geo cache")?;
    Ok(GeoCache { locations })
}

fn migrate_versioned_geo_cache(value: Value) -> GeoCache {
    let mut locations = BTreeMap::new();
    let Some(ips) = value.get("ips").and_then(Value::as_object) else {
        return GeoCache::default();
    };
    for (ip, entry) in ips {
        let Some(decision) = entry.get("decision") else {
            continue;
        };
        let Some(lat) = number_field(decision, "lat") else {
            continue;
        };
        let Some(lon) = number_field(decision, "lon") else {
            continue;
        };
        locations.insert(
            ip.clone(),
            CachedGeoLocation {
                city: string_field(decision, "city").unwrap_or_else(unknown_string),
                country: string_field(decision, "country").unwrap_or_else(unknown_string),
                isp: string_field(decision, "isp").unwrap_or_else(unknown_string),
                lat,
                lon,
                source: string_field(decision, "geo_source").unwrap_or_else(ip_api_source),
                confidence: string_field(decision, "geo_confidence")
                    .unwrap_or_else(medium_confidence),
                updated_at: number_u64_field(decision, "geo_updated_at").unwrap_or_default(),
            },
        );
    }
    GeoCache { locations }
}

fn save_geo_cache(path: &Path, cache: &GeoCache) -> Result<()> {
    let data =
        serde_json::to_vec_pretty(&cache.locations).context("failed to serialize geo cache")?;
    write_file_atomic(path, &data, 0o600)
}

#[derive(Debug, Deserialize)]
struct IpApiResponse {
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    query: Option<String>,
    #[serde(default)]
    country: Option<String>,
    #[serde(default)]
    city: Option<String>,
    #[serde(default)]
    lat: Option<f64>,
    #[serde(default)]
    lon: Option<f64>,
    #[serde(default)]
    isp: Option<String>,
}

impl IpApiResponse {
    fn into_location(self, now: u64) -> Option<(IpAddr, CachedGeoLocation)> {
        if self.status.as_deref() != Some("success") {
            return None;
        }
        let ip = self.query?.parse::<IpAddr>().ok()?;
        let lat = self.lat?;
        let lon = self.lon?;
        if !lat.is_finite() || !lon.is_finite() {
            return None;
        }
        Some((
            ip,
            CachedGeoLocation {
                city: self.city.unwrap_or_else(unknown_string),
                country: self.country.unwrap_or_else(unknown_string),
                isp: self.isp.unwrap_or_else(unknown_string),
                lat,
                lon,
                source: ip_api_source(),
                confidence: medium_confidence(),
                updated_at: now,
            },
        ))
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct MapNode {
    peer: String,
    ip: String,
    city: String,
    country: String,
    isp: String,
    lat: f64,
    lon: f64,
    geo_source: String,
    geo_confidence: String,
    geo_updated_at: u64,
}

fn string_field(value: &Value, field: &str) -> Option<String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn number_field(value: &Value, field: &str) -> Option<f64> {
    value
        .get(field)
        .and_then(|value| value.as_f64().or_else(|| value.as_str()?.parse().ok()))
}

fn number_u64_field(value: &Value, field: &str) -> Option<u64> {
    value
        .get(field)
        .and_then(|value| value.as_u64().or_else(|| value.as_str()?.parse().ok()))
}

fn unknown_if_empty(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        unknown_string()
    } else {
        trimmed.to_owned()
    }
}

fn unknown_string() -> String {
    "Unknown".to_owned()
}

fn ip_api_source() -> String {
    "ip-api".to_owned()
}

fn medium_confidence() -> String {
    "medium".to_owned()
}

fn now_sec() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_array_seed_records() {
        let candidates = collect_candidates_from_value(
            &json!([
                {"peer": "peer-a", "ip": "203.0.113.10:3030"},
                {"public_key": "peer-b", "addresses": ["[2001:db8::1]:3030", "198.51.100.9"]}
            ]),
            None,
        );

        assert_eq!(
            unique_candidates(candidates),
            vec![
                CandidateNode {
                    peer: "peer-a".to_owned(),
                    ip: "203.0.113.10".parse().unwrap(),
                },
                CandidateNode {
                    peer: "peer-b".to_owned(),
                    ip: "198.51.100.9".parse().unwrap(),
                },
                CandidateNode {
                    peer: "peer-b".to_owned(),
                    ip: "2001:db8::1".parse().unwrap(),
                },
            ]
        );
    }

    #[test]
    fn parses_peer_keyed_seed_records() {
        let candidates = collect_candidates_from_value(
            &json!({
                "peer-a": "203.0.113.10",
                "peer-b": ["198.51.100.9:3030", "198.51.100.9:3030"]
            }),
            None,
        );

        assert_eq!(
            unique_candidates(candidates),
            vec![
                CandidateNode {
                    peer: "peer-a".to_owned(),
                    ip: "203.0.113.10".parse().unwrap(),
                },
                CandidateNode {
                    peer: "peer-b".to_owned(),
                    ip: "198.51.100.9".parse().unwrap(),
                },
            ]
        );
    }

    #[test]
    fn builds_backward_compatible_map_nodes() {
        let candidates = vec![CandidateNode {
            peer: "peer-a".to_owned(),
            ip: "203.0.113.10".parse().unwrap(),
        }];
        let mut cache = GeoCache::default();
        cache.locations.insert(
            "203.0.113.10".to_owned(),
            CachedGeoLocation {
                city: "Test City".to_owned(),
                country: "Testland".to_owned(),
                isp: "Test ISP".to_owned(),
                lat: 1.25,
                lon: 2.5,
                source: ip_api_source(),
                confidence: medium_confidence(),
                updated_at: 1_700_000_000,
            },
        );

        let nodes = build_map_nodes_from_candidates(&candidates, &cache);
        let node = serde_json::to_value(&nodes[0]).unwrap();

        assert_eq!(node["peer"], "peer-a");
        assert_eq!(node["ip"], "203.0.113.10");
        assert_eq!(node["city"], "Test City");
        assert_eq!(node["country"], "Testland");
        assert_eq!(node["isp"], "Test ISP");
        assert_eq!(node["lat"], 1.25);
        assert_eq!(node["lon"], 2.5);
    }
}
