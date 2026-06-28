use crate::config::{NodeLocationChainConfig, NodeLocationsConfig};
use crate::fsutil::write_file_atomic;
use crate::state::AppState;
use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::net::{IpAddr, SocketAddr};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::task::JoinSet;
use tokio::time::sleep;
use tracing::{info, warn};

const IP_API_BATCH_SIZE: usize = 100;
const IPINFO_CONCURRENCY: usize = 16;
const MAP_NODE_RETENTION_SECONDS: u64 = 60 * 60;

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
            &state.config.node_locations,
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
    node_config: &NodeLocationsConfig,
    chain_id: &str,
    chain_config: &NodeLocationChainConfig,
    geo_cache: &mut GeoCache,
    now: u64,
    ttl: Duration,
) -> Result<bool> {
    let candidates = collect_local_file_candidates(chain_config)
        .with_context(|| format!("failed to collect node IP seeds for {chain_id}"))?;
    let manual_resolved =
        load_manual_resolved_locations(&node_config.manual_resolved_dir, chain_id);
    let ips = candidates
        .iter()
        .map(|candidate| candidate.ip)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let lookup_ips = ips
        .iter()
        .copied()
        .filter(|ip| !manual_resolved.contains_key(ip))
        .filter(|ip| !geo_cache.has_fresh_location(*ip, now, ttl))
        .collect::<Vec<_>>();

    let fetched =
        lookup_ip_api_locations(http, &node_config.ip_api_batch_endpoint, &lookup_ips, now).await;
    let mut cache_changed = false;
    for (ip, mut location) in fetched {
        if let Some(existing) = geo_cache.location(ip) {
            location.ipinfo = existing.ipinfo.clone();
            location.ipinfo_conflict = existing.ipinfo_conflict;
            location.ipinfo_conflict_reason = existing.ipinfo_conflict_reason.clone();
        }
        geo_cache.locations.insert(ip.to_string(), location);
        cache_changed = true;
    }

    let ipinfo_lookup_count = refresh_ipinfo_verification(
        http,
        node_config,
        &ips,
        &manual_resolved,
        geo_cache,
        now,
        ttl,
    )
    .await;
    cache_changed |= ipinfo_lookup_count > 0;
    cache_changed |= refresh_ipinfo_conflicts(&ips, geo_cache);

    let manual_review_count = write_manual_review_files(
        &node_config.manual_review_dir,
        &node_config.manual_resolved_dir,
        chain_id,
        &ips,
        geo_cache,
        &manual_resolved,
        now,
    )?;

    let previous_nodes = match load_existing_map_nodes(&chain_config.output_path) {
        Ok(nodes) => nodes,
        Err(error) => {
            warn!(
                chain_id,
                path = %chain_config.output_path.display(),
                error = ?error,
                "failed to load previous node location map for retention"
            );
            PreviousMapNodes::default()
        }
    };
    let built_nodes = build_map_nodes_from_candidates_with_retention(
        &candidates,
        geo_cache,
        &manual_resolved,
        &previous_nodes,
        now,
    );
    write_map_nodes_atomic(&chain_config.output_path, &built_nodes.nodes)?;

    info!(
        chain_id,
        seed_node_count = candidates.len(),
        unique_ip_count = ips.len(),
        ip_api_lookup_count = lookup_ips.len(),
        ipinfo_lookup_count,
        manual_resolved_count = manual_resolved.len(),
        manual_review_count,
        retained_node_count = built_nodes.retained_node_count,
        mapped_node_count = built_nodes.nodes.len(),
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

async fn refresh_ipinfo_verification(
    http: &reqwest::Client,
    config: &NodeLocationsConfig,
    ips: &[IpAddr],
    manual_resolved: &BTreeMap<IpAddr, ManualResolvedIp>,
    geo_cache: &mut GeoCache,
    now: u64,
    ttl: Duration,
) -> usize {
    let lookup_ips = ips
        .iter()
        .copied()
        .filter(|ip| !manual_resolved.contains_key(ip))
        .filter(|ip| {
            geo_cache
                .location(*ip)
                .is_some_and(|location| !location.has_fresh_ipinfo(now, ttl))
        })
        .collect::<Vec<_>>();
    if lookup_ips.is_empty() {
        return 0;
    }

    let Some(token) = config.effective_ipinfo_token() else {
        warn!(
            token_env = %config.ipinfo_token_env,
            "ipinfo verification skipped because token is not configured"
        );
        return 0;
    };

    let fetched =
        lookup_ipinfo_lite_locations(http, &config.ipinfo_lite_base_url, &token, &lookup_ips, now)
            .await;
    for (ip, ipinfo) in fetched {
        if let Some(location) = geo_cache.location_mut(ip) {
            location.ipinfo = Some(ipinfo);
        }
    }
    lookup_ips.len()
}

async fn lookup_ipinfo_lite_locations(
    http: &reqwest::Client,
    base_url: &str,
    token: &str,
    ips: &[IpAddr],
    now: u64,
) -> BTreeMap<IpAddr, IpInfoLiteLocation> {
    let mut output = BTreeMap::new();
    for chunk in ips.chunks(IPINFO_CONCURRENCY) {
        let mut tasks = JoinSet::new();
        for ip in chunk {
            let http = http.clone();
            let base_url = base_url.trim_end_matches('/').to_owned();
            let token = token.to_owned();
            let ip = *ip;
            tasks
                .spawn(async move { lookup_ipinfo_lite_one(http, base_url, token, ip, now).await });
        }
        while let Some(result) = tasks.join_next().await {
            match result {
                Ok(Some((ip, location))) => {
                    output.insert(ip, location);
                }
                Ok(None) => {}
                Err(error) => {
                    warn!(error = ?error, "ipinfo lookup task failed");
                }
            }
        }
    }
    output
}

async fn lookup_ipinfo_lite_one(
    http: reqwest::Client,
    base_url: String,
    token: String,
    ip: IpAddr,
    now: u64,
) -> Option<(IpAddr, IpInfoLiteLocation)> {
    let mut url = match reqwest::Url::parse(&format!("{base_url}/{ip}")) {
        Ok(url) => url,
        Err(error) => {
            warn!(ip = %ip, error = ?error, "failed to build ipinfo lookup URL");
            return None;
        }
    };
    url.query_pairs_mut().append_pair("token", &token);

    let response = match http.get(url).send().await {
        Ok(response) => response,
        Err(error) => {
            warn!(ip = %ip, error = ?error, "ipinfo lookup failed");
            return None;
        }
    };
    if !response.status().is_success() {
        warn!(ip = %ip, status = %response.status(), "ipinfo lookup returned an error");
        return None;
    }
    let raw = match response.json::<IpInfoLiteResponse>().await {
        Ok(raw) => raw,
        Err(error) => {
            warn!(ip = %ip, error = ?error, "failed to decode ipinfo response");
            return None;
        }
    };
    raw.into_location(ip, now).map(|location| (ip, location))
}

fn refresh_ipinfo_conflicts(ips: &[IpAddr], geo_cache: &mut GeoCache) -> bool {
    let mut changed = false;
    for ip in ips {
        let Some(location) = geo_cache.location_mut(*ip) else {
            continue;
        };
        let reason = location.ipinfo_conflict_reason();
        let conflict = reason.is_some();
        if location.ipinfo_conflict != conflict || location.ipinfo_conflict_reason != reason {
            location.ipinfo_conflict = conflict;
            location.ipinfo_conflict_reason = reason;
            changed = true;
        }
    }
    changed
}

fn write_manual_review_files(
    manual_review_dir: &Path,
    manual_resolved_dir: &Path,
    chain_id: &str,
    ips: &[IpAddr],
    geo_cache: &GeoCache,
    manual_resolved: &BTreeMap<IpAddr, ManualResolvedIp>,
    now: u64,
) -> Result<usize> {
    let chain_review_dir = manual_review_dir.join(chain_id);
    let mut active_files = BTreeSet::new();

    for ip in ips {
        if manual_resolved.contains_key(ip) {
            continue;
        }
        let Some(location) = geo_cache.location(*ip) else {
            continue;
        };
        if !location.ipinfo_conflict {
            continue;
        }
        let Some(ipinfo) = &location.ipinfo else {
            continue;
        };

        let file_name = manual_ip_file_name(*ip);
        let review_path = chain_review_dir.join(&file_name);
        let manual_path = manual_resolved_dir.join(chain_id).join(&file_name);
        let entry = ManualReviewEntry {
            chain_id: chain_id.to_owned(),
            ip: ip.to_string(),
            detected_at: now,
            reason: location
                .ipinfo_conflict_reason
                .clone()
                .unwrap_or_else(|| "ip-api/ipinfo mismatch".to_owned()),
            ip_api: ReviewIpApiLocation::from(location),
            ipinfo: ipinfo.clone(),
            manual_resolved_path: manual_path.display().to_string(),
        };
        let data =
            serde_json::to_vec_pretty(&entry).context("failed to serialize manual review entry")?;
        write_file_atomic(&review_path, &data, 0o644)?;
        active_files.insert(file_name);
    }

    remove_stale_manual_review_files(&chain_review_dir, &active_files)?;
    Ok(active_files.len())
}

fn remove_stale_manual_review_files(dir: &Path, active_files: &BTreeSet<String>) -> Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(dir).with_context(|| format!("failed to read {}", dir.display()))? {
        let entry = entry.with_context(|| format!("failed to read {}", dir.display()))?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let Some(file_name) = path
            .file_name()
            .and_then(|file_name| file_name.to_str())
            .map(str::to_owned)
        else {
            continue;
        };
        if !active_files.contains(&file_name) {
            fs::remove_file(&path)
                .with_context(|| format!("failed to remove stale {}", path.display()))?;
        }
    }
    Ok(())
}

fn load_manual_resolved_locations(
    manual_resolved_dir: &Path,
    chain_id: &str,
) -> BTreeMap<IpAddr, ManualResolvedIp> {
    let chain_dir = manual_resolved_dir.join(chain_id);
    let mut output = BTreeMap::new();
    let Ok(entries) = fs::read_dir(&chain_dir) else {
        return output;
    };
    for entry in entries {
        let Ok(entry) = entry else {
            continue;
        };
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let body = match fs::read_to_string(&path) {
            Ok(body) => body,
            Err(error) => {
                warn!(path = %path.display(), error = ?error, "failed to read manual resolved IP");
                continue;
            }
        };
        let manual = match serde_json::from_str::<ManualResolvedIp>(&body) {
            Ok(manual) => manual,
            Err(error) => {
                warn!(path = %path.display(), error = ?error, "failed to parse manual resolved IP");
                continue;
            }
        };
        if !manual.geo.latitude.is_finite() || !manual.geo.longitude.is_finite() {
            warn!(path = %path.display(), ip = %manual.ip, "manual resolved IP has invalid coordinates");
            continue;
        }
        output.insert(manual.ip, manual);
    }
    output
}

#[cfg(test)]
fn build_map_nodes_from_candidates(
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

fn build_map_nodes_from_candidates_with_retention(
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

fn write_map_nodes_atomic(path: &Path, nodes: &[MapNode]) -> Result<()> {
    let data = serde_json::to_vec_pretty(nodes).context("failed to serialize map nodes")?;
    write_file_atomic(path, &data, 0o644)
}

fn load_existing_map_nodes(path: &Path) -> Result<PreviousMapNodes> {
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
struct BuiltMapNodes {
    nodes: Vec<MapNode>,
    retained_node_count: usize,
}

#[derive(Debug, Default)]
struct PreviousMapNodes {
    nodes: Vec<MapNode>,
    updated_at: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CandidateNode {
    peer: String,
    ip: IpAddr,
}

impl CandidateNode {
    fn key(&self) -> (String, String) {
        (self.peer_key(), self.ip.to_string().to_ascii_lowercase())
    }

    fn peer_key(&self) -> String {
        self.peer.to_ascii_lowercase()
    }
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

    fn location_mut(&mut self, ip: IpAddr) -> Option<&mut CachedGeoLocation> {
        self.locations.get_mut(&ip.to_string())
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
    #[serde(default)]
    country_code: Option<String>,
    #[serde(default = "unknown_string")]
    isp: String,
    #[serde(default)]
    asn: Option<String>,
    #[serde(default)]
    as_name: Option<String>,
    lat: f64,
    lon: f64,
    #[serde(default = "ip_api_source")]
    source: String,
    #[serde(default = "medium_confidence")]
    confidence: String,
    #[serde(default)]
    updated_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ipinfo: Option<IpInfoLiteLocation>,
    #[serde(default, skip_serializing_if = "is_false")]
    ipinfo_conflict: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ipinfo_conflict_reason: Option<String>,
}

impl CachedGeoLocation {
    fn has_coordinates(&self) -> bool {
        self.lat.is_finite() && self.lon.is_finite()
    }

    fn is_fresh(&self, now: u64, ttl: Duration) -> bool {
        now.saturating_sub(self.updated_at) < ttl.as_secs()
    }

    fn has_fresh_ipinfo(&self, now: u64, ttl: Duration) -> bool {
        self.ipinfo
            .as_ref()
            .is_some_and(|ipinfo| now.saturating_sub(ipinfo.updated_at) < ttl.as_secs())
    }

    fn ipinfo_conflict_reason(&self) -> Option<String> {
        let ipinfo = self.ipinfo.as_ref()?;
        if let (Some(ip_api_code), Some(ipinfo_code)) = (
            normalized_code(&self.country_code),
            normalized_code(&ipinfo.country_code),
        ) && ip_api_code != ipinfo_code
        {
            return Some(format!(
                "country_code mismatch: ip-api={ip_api_code}, ipinfo={ipinfo_code}"
            ));
        }

        let ip_api_country = normalized_name(&self.country);
        let ipinfo_country = normalized_name(&ipinfo.country);
        if let (Some(ip_api_country), Some(ipinfo_country)) = (ip_api_country, ipinfo_country)
            && ip_api_country != ipinfo_country
        {
            return Some(format!(
                "country mismatch: ip-api={}, ipinfo={}",
                self.country, ipinfo.country
            ));
        }

        None
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
                country_code: string_field(decision, "country_code"),
                isp: string_field(decision, "isp").unwrap_or_else(unknown_string),
                asn: None,
                as_name: None,
                lat,
                lon,
                source: string_field(decision, "geo_source").unwrap_or_else(ip_api_source),
                confidence: string_field(decision, "geo_confidence")
                    .unwrap_or_else(medium_confidence),
                updated_at: number_u64_field(decision, "geo_updated_at").unwrap_or_default(),
                ipinfo: None,
                ipinfo_conflict: false,
                ipinfo_conflict_reason: None,
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
    #[serde(default, rename = "countryCode")]
    country_code: Option<String>,
    #[serde(default)]
    city: Option<String>,
    #[serde(default)]
    lat: Option<f64>,
    #[serde(default)]
    lon: Option<f64>,
    #[serde(default)]
    isp: Option<String>,
    #[serde(default, rename = "as")]
    as_text: Option<String>,
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
                country_code: self.country_code.and_then(trimmed_non_empty),
                isp: self.isp.unwrap_or_else(unknown_string),
                asn: self.as_text.as_deref().and_then(parse_asn),
                as_name: self.as_text.as_deref().and_then(parse_as_name),
                lat,
                lon,
                source: ip_api_source(),
                confidence: medium_confidence(),
                updated_at: now,
                ipinfo: None,
                ipinfo_conflict: false,
                ipinfo_conflict_reason: None,
            },
        ))
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct IpInfoLiteLocation {
    #[serde(default)]
    asn: Option<String>,
    #[serde(default)]
    as_name: Option<String>,
    #[serde(default)]
    as_domain: Option<String>,
    #[serde(default)]
    country_code: Option<String>,
    #[serde(default = "unknown_string")]
    country: String,
    #[serde(default)]
    continent_code: Option<String>,
    #[serde(default)]
    continent: Option<String>,
    #[serde(default)]
    updated_at: u64,
}

#[derive(Debug, Deserialize)]
struct IpInfoLiteResponse {
    #[serde(default)]
    ip: Option<String>,
    #[serde(default)]
    asn: Option<String>,
    #[serde(default)]
    as_name: Option<String>,
    #[serde(default)]
    as_domain: Option<String>,
    #[serde(default)]
    country_code: Option<String>,
    #[serde(default)]
    country: Option<String>,
    #[serde(default)]
    continent_code: Option<String>,
    #[serde(default)]
    continent: Option<String>,
}

impl IpInfoLiteResponse {
    fn into_location(self, requested_ip: IpAddr, now: u64) -> Option<IpInfoLiteLocation> {
        if let Some(response_ip) = &self.ip
            && response_ip
                .parse::<IpAddr>()
                .ok()
                .is_some_and(|ip| ip != requested_ip)
        {
            warn!(
                requested_ip = %requested_ip,
                response_ip,
                "ipinfo response IP did not match request"
            );
            return None;
        }

        Some(IpInfoLiteLocation {
            asn: self.asn.and_then(trimmed_non_empty),
            as_name: self.as_name.and_then(trimmed_non_empty),
            as_domain: self.as_domain.and_then(trimmed_non_empty),
            country_code: self.country_code.and_then(trimmed_non_empty),
            country: self
                .country
                .map_or_else(unknown_string, |country| unknown_if_empty(&country)),
            continent_code: self.continent_code.and_then(trimmed_non_empty),
            continent: self.continent.and_then(trimmed_non_empty),
            updated_at: now,
        })
    }
}

#[derive(Clone, Debug, Deserialize)]
struct ManualResolvedIp {
    ip: IpAddr,
    geo: ManualGeo,
    #[serde(default, rename = "as")]
    as_info: Option<ManualAs>,
    #[serde(default)]
    updated_at: Option<u64>,
}

#[derive(Clone, Debug, Deserialize)]
struct ManualGeo {
    #[serde(default = "unknown_string")]
    city: String,
    #[serde(default = "unknown_string")]
    country: String,
    latitude: f64,
    longitude: f64,
}

#[derive(Clone, Debug, Deserialize)]
struct ManualAs {
    #[serde(default = "unknown_string")]
    name: String,
}

#[derive(Debug, Serialize)]
struct ManualReviewEntry {
    chain_id: String,
    ip: String,
    detected_at: u64,
    reason: String,
    ip_api: ReviewIpApiLocation,
    ipinfo: IpInfoLiteLocation,
    manual_resolved_path: String,
}

#[derive(Debug, Serialize)]
struct ReviewIpApiLocation {
    city: String,
    country: String,
    country_code: Option<String>,
    isp: String,
    asn: Option<String>,
    as_name: Option<String>,
    latitude: f64,
    longitude: f64,
    updated_at: u64,
}

impl From<&CachedGeoLocation> for ReviewIpApiLocation {
    fn from(location: &CachedGeoLocation) -> Self {
        Self {
            city: location.city.clone(),
            country: location.country.clone(),
            country_code: location.country_code.clone(),
            isp: location.isp.clone(),
            asn: location.asn.clone(),
            as_name: location.as_name.clone(),
            latitude: location.lat,
            longitude: location.lon,
            updated_at: location.updated_at,
        }
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
    #[serde(default, skip_serializing_if = "is_zero")]
    last_seen_at: u64,
}

impl MapNode {
    fn from_cached_location(
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

    fn from_manual(candidate: &CandidateNode, manual: &ManualResolvedIp, now: u64) -> Self {
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

    fn key(&self) -> (String, String) {
        (self.peer_key(), self.ip.to_ascii_lowercase())
    }

    fn peer_key(&self) -> String {
        self.peer.to_ascii_lowercase()
    }

    fn is_retained(&self, now: u64, fallback_seen_at: Option<u64>) -> bool {
        let last_seen_at = if self.last_seen_at == 0 {
            fallback_seen_at.unwrap_or_default()
        } else {
            self.last_seen_at
        };
        last_seen_at != 0 && now.saturating_sub(last_seen_at) < MAP_NODE_RETENTION_SECONDS
    }
}

fn is_zero(value: &u64) -> bool {
    *value == 0
}

fn string_field(value: &Value, field: &str) -> Option<String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn trimmed_non_empty(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_owned())
    }
}

fn parse_asn(value: &str) -> Option<String> {
    value
        .split_whitespace()
        .next()
        .map(str::trim)
        .filter(|asn| asn.starts_with("AS") && asn.len() > 2)
        .map(str::to_owned)
}

fn parse_as_name(value: &str) -> Option<String> {
    let mut parts = value.splitn(2, char::is_whitespace);
    let _asn = parts.next()?;
    parts
        .next()
        .and_then(|name| trimmed_non_empty(name.to_owned()))
}

fn normalized_code(value: &Option<String>) -> Option<String> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_ascii_uppercase)
}

fn normalized_name(value: &str) -> Option<String> {
    let normalized = value.trim().to_ascii_lowercase();
    if normalized.is_empty() || normalized == "unknown" {
        return None;
    }

    let normalized = normalized
        .strip_prefix("the ")
        .unwrap_or(&normalized)
        .trim();
    Some(match normalized {
        "netherland" | "netherlands" => "netherlands".to_owned(),
        _ => normalized.to_owned(),
    })
}

fn manual_ip_file_name(ip: IpAddr) -> String {
    let safe_ip = ip
        .to_string()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '.' || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    format!("{safe_ip}.json")
}

fn is_false(value: &bool) -> bool {
    !*value
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

    fn cached_location(city: &str, country: &str, country_code: &str) -> CachedGeoLocation {
        CachedGeoLocation {
            city: city.to_owned(),
            country: country.to_owned(),
            country_code: Some(country_code.to_owned()),
            isp: "Test ISP".to_owned(),
            asn: Some("AS64500".to_owned()),
            as_name: Some("Test ISP".to_owned()),
            lat: 1.25,
            lon: 2.5,
            source: ip_api_source(),
            confidence: medium_confidence(),
            updated_at: 1_700_000_000,
            ipinfo: None,
            ipinfo_conflict: false,
            ipinfo_conflict_reason: None,
        }
    }

    fn previous_map_node(peer: &str, ip: &str, last_seen_at: u64) -> MapNode {
        MapNode {
            peer: peer.to_owned(),
            ip: ip.to_owned(),
            city: "Previous City".to_owned(),
            country: "Previousland".to_owned(),
            isp: "Previous ISP".to_owned(),
            lat: 3.0,
            lon: 4.0,
            geo_source: ip_api_source(),
            geo_confidence: medium_confidence(),
            geo_updated_at: 1_700_000_000,
            last_seen_at,
        }
    }

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
    fn parses_resolver_full_validator_records() {
        let candidates = collect_candidates_from_value(
            &json!({
                "validators": [
                    {
                        "validator_public_key": "peer-a",
                        "resolution": {
                            "status": "resolved",
                            "addresses": [
                                {"ip": "203.0.113.10", "port": 30313, "version": "udp4"}
                            ]
                        }
                    }
                ]
            }),
            None,
        );

        assert_eq!(
            unique_candidates(candidates),
            vec![CandidateNode {
                peer: "peer-a".to_owned(),
                ip: "203.0.113.10".parse().unwrap(),
            }]
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
            cached_location("Test City", "Testland", "TL"),
        );

        let nodes = build_map_nodes_from_candidates(&candidates, &cache, &BTreeMap::new());
        let node = serde_json::to_value(&nodes[0]).unwrap();

        assert_eq!(node["peer"], "peer-a");
        assert_eq!(node["ip"], "203.0.113.10");
        assert_eq!(node["city"], "Test City");
        assert_eq!(node["country"], "Testland");
        assert_eq!(node["isp"], "Test ISP");
        assert_eq!(node["lat"], 1.25);
        assert_eq!(node["lon"], 2.5);
    }

    #[test]
    fn manual_resolved_ip_overrides_cached_location() {
        let ip = "203.0.113.10".parse().unwrap();
        let candidates = vec![CandidateNode {
            peer: "peer-a".to_owned(),
            ip,
        }];
        let mut cache = GeoCache::default();
        cache.locations.insert(
            "203.0.113.10".to_owned(),
            cached_location("Wrong City", "Wrongland", "WL"),
        );
        let manual_resolved = BTreeMap::from([(
            ip,
            ManualResolvedIp {
                ip,
                geo: ManualGeo {
                    city: "Manual City".to_owned(),
                    country: "Manualia".to_owned(),
                    latitude: 9.0,
                    longitude: 10.0,
                },
                as_info: Some(ManualAs {
                    name: "Manual ISP".to_owned(),
                }),
                updated_at: Some(1_800_000_000),
            },
        )]);

        let nodes = build_map_nodes_from_candidates(&candidates, &cache, &manual_resolved);
        let node = serde_json::to_value(&nodes[0]).unwrap();

        assert_eq!(node["city"], "Manual City");
        assert_eq!(node["country"], "Manualia");
        assert_eq!(node["isp"], "Manual ISP");
        assert_eq!(node["lat"], 9.0);
        assert_eq!(node["lon"], 10.0);
        assert_eq!(node["geo_source"], "manual");
        assert_eq!(node["geo_confidence"], "manual");
    }

    #[test]
    fn ipinfo_country_conflict_holds_node_for_manual_review() {
        let ip = "203.0.113.10".parse().unwrap();
        let candidates = vec![CandidateNode {
            peer: "peer-a".to_owned(),
            ip,
        }];
        let mut cache = GeoCache::default();
        let mut location = cached_location("Test City", "United States", "US");
        location.ipinfo = Some(IpInfoLiteLocation {
            asn: Some("AS64500".to_owned()),
            as_name: Some("Test ISP".to_owned()),
            as_domain: Some("example.net".to_owned()),
            country_code: Some("BR".to_owned()),
            country: "Brazil".to_owned(),
            continent_code: Some("SA".to_owned()),
            continent: Some("South America".to_owned()),
            updated_at: 1_700_000_001,
        });
        cache.locations.insert("203.0.113.10".to_owned(), location);

        assert!(refresh_ipinfo_conflicts(&[ip], &mut cache));
        assert!(cache.location(ip).unwrap().ipinfo_conflict);

        let nodes = build_map_nodes_from_candidates(&candidates, &cache, &BTreeMap::new());
        assert!(nodes.is_empty());
    }

    #[test]
    fn retains_previous_map_node_for_transient_missing_candidate() {
        let previous_nodes = PreviousMapNodes {
            nodes: vec![previous_map_node("peer-a", "203.0.113.10", 1_700_000_000)],
            updated_at: None,
        };

        let built = build_map_nodes_from_candidates_with_retention(
            &[],
            &GeoCache::default(),
            &BTreeMap::new(),
            &previous_nodes,
            1_700_000_300,
        );

        assert_eq!(built.retained_node_count, 1);
        assert_eq!(built.nodes.len(), 1);
        assert_eq!(built.nodes[0].peer, "peer-a");
    }

    #[test]
    fn expires_previous_map_node_after_retention_window() {
        let previous_nodes = PreviousMapNodes {
            nodes: vec![previous_map_node("peer-a", "203.0.113.10", 1_700_000_000)],
            updated_at: None,
        };

        let built = build_map_nodes_from_candidates_with_retention(
            &[],
            &GeoCache::default(),
            &BTreeMap::new(),
            &previous_nodes,
            1_700_003_601,
        );

        assert_eq!(built.retained_node_count, 0);
        assert!(built.nodes.is_empty());
    }

    #[test]
    fn retention_uses_file_timestamp_for_legacy_nodes_without_last_seen_at() {
        let legacy_node: MapNode = serde_json::from_value(json!({
            "peer": "peer-a",
            "ip": "203.0.113.10",
            "city": "Previous City",
            "country": "Previousland",
            "isp": "Previous ISP",
            "lat": 3.0,
            "lon": 4.0,
            "geo_source": ip_api_source(),
            "geo_confidence": medium_confidence(),
            "geo_updated_at": 1_700_000_000
        }))
        .unwrap();
        let previous_nodes = PreviousMapNodes {
            nodes: vec![legacy_node],
            updated_at: Some(1_700_000_000),
        };

        let built = build_map_nodes_from_candidates_with_retention(
            &[],
            &GeoCache::default(),
            &BTreeMap::new(),
            &previous_nodes,
            1_700_000_300,
        );

        assert_eq!(built.retained_node_count, 1);
        assert_eq!(built.nodes.len(), 1);
        assert_eq!(built.nodes[0].peer, "peer-a");
        assert_eq!(built.nodes[0].last_seen_at, 0);
    }

    #[test]
    fn ipinfo_conflict_does_not_retain_previous_map_node_for_same_peer() {
        let ip = "203.0.113.10".parse().unwrap();
        let candidates = vec![CandidateNode {
            peer: "peer-a".to_owned(),
            ip,
        }];
        let mut cache = GeoCache::default();
        let mut location = cached_location("Test City", "United States", "US");
        location.ipinfo = Some(IpInfoLiteLocation {
            asn: Some("AS64500".to_owned()),
            as_name: Some("Test ISP".to_owned()),
            as_domain: Some("example.net".to_owned()),
            country_code: Some("BR".to_owned()),
            country: "Brazil".to_owned(),
            continent_code: Some("SA".to_owned()),
            continent: Some("South America".to_owned()),
            updated_at: 1_700_000_001,
        });
        cache.locations.insert("203.0.113.10".to_owned(), location);
        assert!(refresh_ipinfo_conflicts(&[ip], &mut cache));

        let previous_nodes = PreviousMapNodes {
            nodes: vec![previous_map_node("peer-a", "203.0.113.10", 1_700_000_000)],
            updated_at: None,
        };
        let built = build_map_nodes_from_candidates_with_retention(
            &candidates,
            &cache,
            &BTreeMap::new(),
            &previous_nodes,
            1_700_000_300,
        );

        assert_eq!(built.retained_node_count, 0);
        assert!(built.nodes.is_empty());
    }

    #[test]
    fn netherlands_country_aliases_do_not_create_manual_review() {
        let ip = "203.0.113.10".parse().unwrap();
        let candidates = vec![CandidateNode {
            peer: "peer-a".to_owned(),
            ip,
        }];
        let mut cache = GeoCache::default();
        let mut location = cached_location("Amsterdam", "Netherland", "");
        location.country_code = None;
        location.ipinfo = Some(IpInfoLiteLocation {
            asn: Some("AS64500".to_owned()),
            as_name: Some("Test ISP".to_owned()),
            as_domain: Some("example.net".to_owned()),
            country_code: None,
            country: "The Netherlands".to_owned(),
            continent_code: Some("EU".to_owned()),
            continent: Some("Europe".to_owned()),
            updated_at: 1_700_000_001,
        });
        cache.locations.insert("203.0.113.10".to_owned(), location);

        assert!(!refresh_ipinfo_conflicts(&[ip], &mut cache));
        assert!(!cache.location(ip).unwrap().ipinfo_conflict);
        assert_eq!(
            normalized_name("The Netherlands"),
            normalized_name("Netherlands")
        );

        let nodes = build_map_nodes_from_candidates(&candidates, &cache, &BTreeMap::new());
        assert_eq!(nodes.len(), 1);
    }

    #[test]
    fn manual_review_file_name_is_ipv6_safe() {
        assert_eq!(
            manual_ip_file_name("2804:388:425b:c8b:10d3:81b7:646c:9b32".parse().unwrap()),
            "2804_388_425b_c8b_10d3_81b7_646c_9b32.json"
        );
    }
}
