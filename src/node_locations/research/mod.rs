//! Persistent, demand-driven geolocation. Polling seed files does not imply network I/O.
mod globalping;
mod model;
#[cfg(test)]
mod operators;
mod sources;
#[cfg(test)]
mod tests;

use super::{geo_cache::GeoCache, manual_review::ManualResolvedIp};
use crate::{config::NodeLocationsConfig, fsutil::write_file_atomic};
use anyhow::{Context, Result, bail};
pub(crate) use model::ResearchConfig;
use model::{DAY, Entry, Observation, Store, decide};
use serde_json::Value;
use std::{collections::BTreeMap, net::IpAddr, path::PathBuf, time::Duration};
use tracing::{info, warn};

pub(crate) struct Engine {
    config: ResearchConfig,
    store: Store,
    directory: PathBuf,
    path: PathBuf,
    database: Option<maxminddb::Reader<Vec<u8>>>,
    feeds: BTreeMap<String, Vec<sources::FeedRow>>,
}
impl Engine {
    pub(crate) fn open(config: &NodeLocationsConfig) -> Result<Self> {
        let path = config.geo_cache_path.with_extension("research.json");
        let directory = config.geo_cache_path.with_extension("research-data");
        let store = if path.exists() {
            serde_json::from_slice::<Store>(&std::fs::read(&path)?)
                .context("research state is unreadable; refusing to restart network research")?
        } else {
            Store {
                version: 1,
                ..Store::default()
            }
        };
        if store.version != 1 {
            bail!("unsupported research state version");
        }
        std::fs::create_dir_all(&directory)?;
        Ok(Self {
            config: config.research.clone(),
            store,
            directory,
            path,
            database: None,
            feeds: BTreeMap::new(),
        })
    }
    fn database_path(&self) -> PathBuf {
        self.directory.join("dbip-city-lite.mmdb")
    }
    fn save(&self) -> Result<()> {
        write_file_atomic(&self.path, &serde_json::to_vec(&self.store)?, 0o600)
    }
    fn reserve(&mut self, source: &str, now: u64) -> Result<bool> {
        if !self.store.budget.reserve(source, now, &self.config) {
            return Ok(false);
        }
        // Write before sending, including failed calls. Crash/restart cannot reset the quota.
        self.save()?;
        Ok(true)
    }
    fn rate_headers(&mut self, source: &str, response: &reqwest::Response, now: u64) -> Result<()> {
        let now = now.max(crate::timeutil::now_sec());
        let seconds = |name| {
            response
                .headers()
                .get(name)
                .and_then(|x| x.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
        };
        let delay = if response.status().as_u16() == 429 {
            seconds("retry-after")
                .or_else(|| seconds("x-ttl"))
                .unwrap_or(DAY)
        } else if response.headers().get("x-rl").is_some_and(|v| v == "0") {
            seconds("x-ttl").unwrap_or(60)
        } else {
            0
        };
        if delay > 0 {
            if source.starts_with("globalping-") {
                for name in ["globalping-read", "globalping-create"] {
                    let until = self.store.budget.not_before.entry(name.into()).or_default();
                    *until = (*until).max(now.saturating_add(delay));
                }
            }
            let until = self
                .store
                .budget
                .not_before
                .entry(source.into())
                .or_default();
            *until = (*until).max(now.saturating_add(delay));
            self.save()?;
        }
        Ok(())
    }
    fn persist_entry(&mut self, ip: IpAddr, entry: Entry) -> Result<()> {
        self.store.entries.insert(ip.to_string(), entry);
        self.save()
    }

    pub(super) async fn refresh(
        &mut self,
        config: &NodeLocationsConfig,
        ips: &[IpAddr],
        manual: &BTreeMap<IpAddr, ManualResolvedIp>,
        cache: &mut GeoCache,
        now: u64,
    ) -> Result<bool> {
        let before = self.store.budget.total_requests;
        let mut pending = Vec::new();
        let mut changed = false;
        for ip in ips.iter().filter(|ip| !manual.contains_key(ip)) {
            let entry = self.store.entries.entry(ip.to_string()).or_default();
            entry.last_seen_at = now;
            if entry.due(now, self.config.reuse_days) {
                pending.push(*ip);
            } else if let Some(point) = &entry.decision {
                // Reconstruct map cache after a crash between the two atomic file writes.
                let selected = point.cached(&entry.confidence);
                if cache.location(*ip).is_none_or(|old| {
                    old.source != selected.source
                        || old.updated_at != selected.updated_at
                        || old.city != selected.city
                        || old.country != selected.country
                        || old.country_code != selected.country_code
                        || old.isp != selected.isp
                        || old.confidence != selected.confidence
                        || old.lat != selected.lat
                        || old.lon != selected.lon
                }) {
                    cache.locations.insert(ip.to_string(), selected);
                    changed = true;
                }
            }
        }
        pending.sort_by_key(|ip| {
            let e = &self.store.entries[&ip.to_string()];
            (e.attempts, e.next_attempt_at, *ip)
        });
        pending.truncate(self.config.max_ips_per_cycle.clamp(1, 500));
        if pending.is_empty() {
            return Ok(changed);
        }
        let max_age = self.config.reuse_days.clamp(1, 3650) * DAY;
        for ip in &pending {
            let entry = self.store.entries.get_mut(&ip.to_string()).unwrap();
            if entry.completed_at > 0 {
                *entry = Entry {
                    last_seen_at: now,
                    ..Entry::default()
                };
            }
            if entry.confidence == "disputed"
                && now.saturating_sub(entry.generation_started_at) >= 7 * DAY
            {
                entry.observations.remove("ip-api");
                entry.observations.remove("ipwho.is");
                entry.secondary_checked = false;
                entry.measurements.clear();
                entry.globalping = None;
                entry.generation_started_at = now;
            }
            if entry.generation_started_at == 0 {
                entry.generation_started_at = now;
            }
            // Migration uses an existing recent PRIMARY observation once. Old majority-vote
            // decisions aren't original ip-api observations and must be looked up afresh.
            if entry.observations.is_empty()
                && let Some(old) = cache.location(*ip)
                && old.source == "ip-api"
                && Observation::from_cached(old).valid()
                && now.saturating_sub(old.updated_at) < max_age
            {
                entry
                    .observations
                    .insert("ip-api".into(), Observation::from_cached(old));
            }
        }
        // Preserve existing third-source answers as evidence, not as the primary truth.
        // This also prevents a migration from erasing known country disagreements.
        for ip in &pending {
            let entry = self.store.entries.get_mut(&ip.to_string()).unwrap();
            if entry.attempts == 0
                && !entry.observations.contains_key("ipwho.is")
                && let Some(old) = cache.location(*ip)
                && old.source == "ipwho.is"
                && Observation::from_cached(old).valid()
                && now.saturating_sub(old.updated_at) < max_age
            {
                entry
                    .observations
                    .insert("ipwho.is".into(), Observation::from_cached(old));
                entry.secondary_checked = true;
            }
        }
        self.save()?;
        let primary = pending
            .iter()
            .copied()
            .filter(|ip| {
                !self.store.entries[&ip.to_string()]
                    .observations
                    .contains_key("ip-api")
            })
            .collect::<Vec<_>>();
        for chunk in primary.chunks(100) {
            let sent_at = crate::timeutil::now_sec().max(now);
            if !self.reserve("ip-api", sent_at)? {
                break;
            }
            let response = crate::http::shared_client()
                .post(&config.ip_api_batch_endpoint)
                .timeout(Duration::from_secs(20))
                .json(&chunk.iter().map(ToString::to_string).collect::<Vec<_>>())
                .send()
                .await;
            if let Ok(response) = response {
                self.rate_headers("ip-api", &response, sent_at)?;
                if response.status().is_success()
                    && let Ok(rows) = crate::http::json_within::<Vec<Value>>(
                        response,
                        crate::http::MAX_GEO_RESPONSE_BYTES,
                    )
                    .await
                {
                    for row in rows {
                        if let Some(ip) = row
                            .get("query")
                            .and_then(Value::as_str)
                            .and_then(|s| s.parse::<IpAddr>().ok())
                            && chunk.contains(&ip)
                            && let Some(point) = sources::observation(&row, "ip-api", sent_at)
                        {
                            self.store
                                .entries
                                .get_mut(&ip.to_string())
                                .unwrap()
                                .observations
                                .insert("ip-api".into(), point);
                        }
                    }
                }
            }
            self.save()?;
        }
        let has_asn = |asn| {
            pending.iter().any(|ip| {
                self.store.entries[&ip.to_string()]
                    .observations
                    .values()
                    .any(|o| o.asn.as_deref() == Some(asn))
            })
        };
        if let Err(error) = self
            .assets(
                now,
                has_asn("AS24940"),
                has_asn("AS396356") || has_asn("AS262287"),
            )
            .await
        {
            warn!(error=%error,"research assets unavailable; keeping existing locations");
        }
        let probes = if self.config.network_measurements {
            self.global_probes(now).await?
        } else {
            vec![]
        };
        for ip in pending {
            let mut entry = self.store.entries[&ip.to_string()].clone();
            if let Some(reader) = &self.database
                && let Some(mut point) = sources::database_observation(reader, ip, now)
            {
                if let Some(primary) = entry.observations.get("ip-api") {
                    point.asn = primary.asn.clone();
                    point.isp = primary.isp.clone();
                }
                entry.observations.insert("dbip".into(), point);
            }
            decide(&mut entry);
            self.apply_feed(ip, &mut entry);
            // One additional database lookup only for disagreements or a missing primary.
            if (!entry.reasons.is_empty() || !entry.observations.contains_key("ip-api"))
                && !entry.secondary_checked
            {
                let url = format!("{}/{ip}", config.tiebreak_base_url.trim_end_matches('/'));
                if let Some(body) = self
                    .get_bytes(
                        "ipwho.is",
                        &url,
                        crate::timeutil::now_sec().max(now),
                        64 * 1024,
                        20,
                    )
                    .await?
                    && let Ok(value) = serde_json::from_slice::<Value>(&body)
                    && value
                        .get("ip")
                        .and_then(Value::as_str)
                        .and_then(|s| s.parse::<IpAddr>().ok())
                        == Some(ip)
                    && let Some(point) = sources::observation(&value, "ipwho.is", now)
                {
                    entry.observations.insert("ipwho.is".into(), point);
                    entry.secondary_checked = true;
                }
                decide(&mut entry);
            }
            self.apply_feed(ip, &mut entry);
            let locations = globalping::locations(&entry, &probes);
            let wants_measurement = self.config.network_measurements
                && (!locations.is_empty() || entry.globalping.is_some());
            let mut measurement_pending = false;
            if wants_measurement {
                if entry.confidence != "disputed" {
                    entry.confidence = "disputed".into();
                    entry
                        .reasons
                        .push("network measurement requested; metro uncertain".into());
                }
                measurement_pending = self
                    .global_measure(
                        ip,
                        &mut entry,
                        locations,
                        now.max(crate::timeutil::now_sec()),
                    )
                    .await?;
            }
            let measured =
                globalping::apply(ip, &mut entry, now.max(crate::timeutil::now_sec()), max_age);
            let needs_secondary = !entry.reasons.is_empty() && !entry.secondary_checked;
            let complete = measured
                || (entry.confidence != "disputed"
                    && entry.decision.is_some()
                    && entry.observations.contains_key("ip-api")
                    && (entry.observations.contains_key("dbip") || !self.config.download_database)
                    && !needs_secondary
                    && !measurement_pending);
            if complete {
                entry.completed_at = now;
                entry.next_attempt_at = now.saturating_add(max_age);
            } else {
                // A fully researched disagreement needs new data, not the same calls hourly.
                if entry.confidence == "disputed" && entry.secondary_checked && !measurement_pending
                {
                    entry.attempts = entry.attempts.max(4);
                }
                entry.retry(now);
            }
            if entry.decision.is_none()
                && let Some(old) = cache.location_mut(ip)
            {
                old.confidence = "stale".into();
                changed = true;
            }
            if let Some(point) = &entry.decision {
                cache
                    .locations
                    .insert(ip.to_string(), point.cached(&entry.confidence));
                changed = true;
            }
            self.persist_entry(ip, entry)?;
        }
        info!(
            network_requests = self.store.budget.total_requests - before,
            total_requests = self.store.budget.total_requests,
            "demand-driven geolocation pass complete"
        );
        Ok(changed)
    }

    pub(super) fn checkpoint(
        &mut self,
        seen: &std::collections::BTreeSet<IpAddr>,
        now: u64,
    ) -> Result<()> {
        let floor = now.saturating_sub(365 * DAY);
        self.store.entries.retain(|ip, entry| {
            entry.last_seen_at >= floor || ip.parse::<IpAddr>().is_ok_and(|ip| seen.contains(&ip))
        });
        if now.saturating_sub(*self.store.assets.get("checkpoint").unwrap_or(&0)) >= DAY {
            self.store.assets.insert("checkpoint".into(), now);
            self.save()?;
        }
        Ok(())
    }

    fn apply_feed(&self, ip: IpAddr, entry: &mut Entry) {
        let Some(base) = &entry.decision else {
            return;
        };
        let provider = match base.asn.as_deref() {
            Some("AS24940") => "hetzner",
            Some("AS396356" | "AS262287") => "latitude",
            _ => return,
        };
        let Some(row) = self
            .feeds
            .get(provider)
            .and_then(|rows| sources::feed_match(rows, ip))
        else {
            return;
        };
        entry.operator_evidence = Some(model::OperatorEvidence {
            url: if provider == "hetzner" {
                "https://www.hetzner.com/geofeed.csv"
            } else {
                "https://geofeed.latitude.sh/"
            }
            .into(),
            prefix: row.network.to_string(),
            country_code: row.code.clone(),
            region: row.region.clone(),
            city: row.city.clone(),
            fetched_at: self
                .store
                .assets
                .get(&format!("{provider}-updated"))
                .copied(),
        });
        // Corroborates city label, not invented coordinates. Don't silently geocode ambiguous names.
        if row.code == base.country_code && row.city.eq_ignore_ascii_case(&base.city) {
            if entry.confidence == "approximate" {
                entry.confidence = "operator_city".into();
            }
        } else {
            entry.confidence = "disputed".into();
            entry.reasons.push(format!(
                "operator geofeed {}: {}, {}, {}",
                row.network, row.code, row.region, row.city
            ));
        }
    }
}
