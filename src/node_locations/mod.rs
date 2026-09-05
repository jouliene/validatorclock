//! Background refresh that turns resolver seed files into the map node files
//! the frontend reads.

mod candidates;
mod fields;
mod geo_cache;
mod ipinfo;
mod manual_review;
mod map_nodes;
mod tiebreak;

#[cfg(test)]
mod tests;

use candidates::collect_local_file_candidates;
use fields::normalized_code;

/// How long an address nobody names any more is kept. Comfortably longer than
/// the lookup TTL, so an address that merely went quiet for a while is not
/// paid for again.
const GEO_CACHE_RETENTION: Duration = Duration::from_secs(30 * 24 * 60 * 60);
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
    let mut any_chain_failed = false;
    let mut seen_ips = BTreeSet::new();

    for chain in &state.config.chains {
        let chain_config = state.config.effective_node_location_chain(&chain.id);
        if !chain_config.enabled {
            continue;
        }

        match refresh_chain_locations(
            &ChainRefresh {
                http,
                node_config: &state.config.node_locations,
                chain_id: &chain.id,
                chain_config: &chain_config,
                now,
                ttl,
            },
            &mut geo_cache,
            &mut seen_ips,
        )
        .await
        {
            Ok(changed) => {
                cache_changed |= changed;
                // The map just published is part of the answer readers get, so
                // that answer is worked out again now rather than waiting for
                // the next chain refresh to notice.
                let _ = state.refresh_ready_snapshot(&chain.id).await;
            }
            Err(error) => {
                any_chain_failed = true;
                warn!(
                    chain_id = %chain.id,
                    error = ?error,
                    "node location refresh failed"
                );
            }
        }
    }

    // Only when every chain reported in: a chain that failed contributed no
    // addresses, and dropping its entries would mean paying for them again.
    if !any_chain_failed {
        cache_changed |= prune_geo_cache(&mut geo_cache, &seen_ips, now);
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

/// Everything one chain's refresh reads, kept together so the call does not
/// grow a queue of positional arguments.
struct ChainRefresh<'a> {
    http: &'a reqwest::Client,
    node_config: &'a NodeLocationsConfig,
    chain_id: &'a str,
    chain_config: &'a NodeLocationChainConfig,
    now: u64,
    ttl: Duration,
}

async fn refresh_chain_locations(
    refresh: &ChainRefresh<'_>,
    geo_cache: &mut GeoCache,
    seen_ips: &mut BTreeSet<std::net::IpAddr>,
) -> Result<bool> {
    let ChainRefresh {
        http,
        node_config,
        chain_id,
        chain_config,
        now,
        ttl,
    } = *refresh;
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
    seen_ips.extend(ips.iter().copied());
    let lookup_ips = ips
        .iter()
        .copied()
        .filter(|ip| !manual_resolved.contains_key(ip))
        .filter(|ip| !geo_cache.has_fresh_location(*ip, now, ttl))
        .collect::<Vec<_>>();

    let fetched =
        lookup_ip_api_locations(&node_config.ip_api_batch_endpoint, &lookup_ips, now).await;
    let mut cache_changed = false;
    let requested = lookup_ips.iter().copied().collect::<BTreeSet<_>>();
    for (ip, mut location) in fetched {
        // The answer says which address each row is for, and the answer is a
        // stranger's. A row for an address nobody asked about is dropped
        // rather than cached: otherwise one reply can seed the map with
        // locations for addresses that were never looked up.
        if !requested.contains(&ip) {
            warn!(ip = %ip, "geo answer names an address that was not asked about");
            continue;
        }
        if let Some(existing) = geo_cache.location(ip) {
            location.ipinfo = existing.ipinfo.clone();
            location.ipinfo_checked_at = existing.ipinfo_checked_at;
            location.ipinfo_conflict = existing.ipinfo_conflict;
            location.ipinfo_conflict_reason = existing.ipinfo_conflict_reason.clone();
            // A third source's answer outlives a routine ip-api refresh, so it
            // is not thrown away and asked for again.
            location.tiebreak = existing.tiebreak.clone();
            // A settled disagreement stays settled unless ip-api has changed
            // its mind about the country.
            location.ipinfo_conflict_settled = existing.ipinfo_conflict_settled
                && normalized_code(&existing.country_code)
                    == normalized_code(&location.country_code);
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

    let auto_resolved_count =
        tiebreak::resolve_conflicts(node_config, &ips, geo_cache, now, ttl).await;
    cache_changed |= auto_resolved_count > 0;

    // Bookkeeping for a person to read. It runs after every external lookup
    // is already paid for, so a filesystem error here must not throw the map
    // away and make the whole cycle happen again.
    let manual_review_count = write_manual_review_files(
        &node_config.manual_review_dir,
        &node_config.manual_resolved_dir,
        chain_id,
        &ips,
        geo_cache,
        &manual_resolved,
        now,
    )
    .unwrap_or_else(|error| {
        warn!(
            chain_id,
            dir = %node_config.manual_review_dir.display(),
            error = ?error,
            "failed to write the manual review files"
        );
        0
    });

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
        auto_resolved_count,
        manual_review_count,
        retained_node_count = built_nodes.retained_node_count,
        mapped_node_count = built_nodes.nodes.len(),
        output_path = %chain_config.output_path.display(),
        "published node location map"
    );

    Ok(cache_changed)
}

/// Entries for addresses no chain names any more, and that nothing has
/// refreshed in a long time, are dropped. Nothing removed them before, so the
/// file only ever grew - and every cycle read and rewrote the whole of it.
fn prune_geo_cache(
    geo_cache: &mut GeoCache,
    seen_ips: &BTreeSet<std::net::IpAddr>,
    now: u64,
) -> bool {
    let floor = now.saturating_sub(GEO_CACHE_RETENTION.as_secs());
    let before = geo_cache.locations.len();

    geo_cache.locations.retain(|ip, location| {
        ip.parse::<std::net::IpAddr>()
            .is_ok_and(|ip| seen_ips.contains(&ip))
            || location.updated_at >= floor
    });

    let dropped = before - geo_cache.locations.len();
    if dropped > 0 {
        info!(
            dropped,
            kept = geo_cache.locations.len(),
            "pruned the geo cache"
        );
    }
    dropped > 0
}

/// ip-api answers only over cleartext on the keyless tier - https returns 403 -
/// so this cannot simply be switched. Saying so once at startup keeps the
/// trade-off visible: the addresses looked up travel in the clear, and the
/// answer arrives from a channel anyone on the path can rewrite. Everything
/// that answer carries is folded, capped and range-checked before it is
/// stored, so a rewritten answer cannot do more than lie about a location.
pub(crate) fn warn_if_geo_lookups_are_cleartext(config: &NodeLocationsConfig) {
    if config.ip_api_batch_endpoint.starts_with("http://") {
        warn!(
            "geo lookups run over cleartext HTTP; the addresses looked up are visible on the path"
        );
    }
}

/// The map's own reader, for a test elsewhere that checks the resolver writes
/// a file this can still make sense of.
#[cfg(test)]
pub(crate) fn candidates_from_value_for_test(
    value: &serde_json::Value,
) -> Vec<(String, String, Option<u64>)> {
    candidates::collect_candidates_from_value(value, None)
        .into_iter()
        .map(|candidate| {
            (
                candidate.peer,
                candidate.ip.to_string(),
                candidate.confirmed_at,
            )
        })
        .collect()
}
