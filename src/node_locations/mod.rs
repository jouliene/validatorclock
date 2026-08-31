//! Background refresh that turns resolver seed files into the map node files
//! the frontend reads.

mod candidates;
mod fields;
mod geo_cache;
mod ipinfo;
mod manual_review;
mod map_nodes;

#[cfg(test)]
mod tests;

use candidates::collect_local_file_candidates;
use geo_cache::{GeoCache, load_geo_cache, lookup_ip_api_locations, save_geo_cache};
use ipinfo::{refresh_ipinfo_conflicts, refresh_ipinfo_verification};
use manual_review::{load_manual_resolved_locations, write_manual_review_files};
use map_nodes::{
    PreviousMapNodes, build_map_nodes_from_candidates_with_retention, load_existing_map_nodes,
    write_map_nodes_atomic,
};

use crate::config::{NodeLocationChainConfig, NodeLocationsConfig};
use crate::state::AppState;
use crate::timeutil::now_sec;
use anyhow::{Context, Result};
use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{info, warn};

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
    let http = crate::http::shared_client();
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
            http,
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
        lookup_ip_api_locations(&node_config.ip_api_batch_endpoint, &lookup_ips, now).await;
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
