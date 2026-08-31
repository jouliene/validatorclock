use crate::fsutil;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::net::IpAddr;
use std::path::Path;
use tracing::warn;

use super::AppState;
use crate::timeutil::{
    SECONDS_PER_DAY, day_index, day_string, now_sec as now_seconds, parse_day_index,
};

const SESSION_TIMEOUT_SECONDS: u64 = 1_800;
const ONLINE_WINDOW_SECONDS: u64 = 120;
const DAY_RETENTION_DAYS: i64 = 31;
const RECORD_RETENTION_DAYS: i64 = 365;
const MAX_VISITOR_RECORDS: usize = 5_000;
const MAX_PUBLIC_ROWS: usize = 1_000;
const GEO_TTL_SECONDS: u64 = 30 * SECONDS_PER_DAY;
const SAVE_INTERVAL_SECONDS: u64 = 15;

#[derive(Debug)]
pub(super) struct VisitorsRuntime {
    disk: VisitorsDisk,
    last_saved: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct VisitorsDisk {
    #[serde(default)]
    visitors: BTreeMap<String, VisitorRecord>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct VisitorRecord {
    #[serde(default)]
    first_seen: u64,
    #[serde(default)]
    last_seen: u64,
    #[serde(default)]
    total_visits: u64,
    #[serde(default)]
    days: BTreeMap<String, u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    geo: Option<VisitorGeo>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct VisitorGeo {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) country: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) country_code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) city: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) isp: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) asn: Option<String>,
    #[serde(default)]
    pub(crate) updated_at: u64,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct PublicVisitors {
    generated_at: u64,
    online_window_seconds: u64,
    session_timeout_seconds: u64,
    known_visitors: u64,
    listed_visitors: u64,
    online_now: u64,
    visitors: Vec<PublicVisitor>,
}

#[derive(Debug, Clone, Serialize)]
struct PublicVisitor {
    ip: String,
    country: Option<String>,
    country_code: Option<String>,
    city: Option<String>,
    isp: Option<String>,
    asn: Option<String>,
    today_visits: u64,
    last_30_days_visits: u64,
    total_visits: u64,
    first_seen: u64,
    last_seen: u64,
    online: bool,
}

pub(super) fn load_initial_runtime(path: &Path) -> VisitorsRuntime {
    let disk = match load_visitors_disk(path) {
        Ok(disk) => disk,
        Err(error) => {
            warn!(
                path = %path.display(),
                error = ?error,
                "failed to load visitor store; starting with empty visitor state"
            );
            VisitorsDisk::default()
        }
    };

    VisitorsRuntime {
        disk,
        last_saved: 0,
    }
}

impl AppState {
    pub(crate) async fn record_visitor(&self, ip: IpAddr) {
        let now = now_seconds();
        let today_index = day_index(now);
        let today = day_string(today_index);

        let snapshot = {
            let mut runtime = self.visitors.lock().await;
            let record = runtime.disk.visitors.entry(ip.to_string()).or_default();
            let starts_visit = record.first_seen == 0
                || now.saturating_sub(record.last_seen) > SESSION_TIMEOUT_SECONDS;
            if record.first_seen == 0 {
                record.first_seen = now;
            }
            if starts_visit {
                record.total_visits = record.total_visits.saturating_add(1);
                let day = record.days.entry(today).or_default();
                *day = day.saturating_add(1);
            }
            record.last_seen = now;

            // Heartbeats only move `last_seen`, so they are flushed on an interval
            // instead of rewriting the whole store on every beat.
            if !starts_visit && now.saturating_sub(runtime.last_saved) < SAVE_INTERVAL_SECONDS {
                return;
            }
            prune_visitors(&mut runtime.disk, today_index);
            runtime.last_saved = now;
            runtime.disk.clone()
        };

        self.save_visitors(&snapshot);
    }

    pub(crate) async fn public_visitors(&self) -> PublicVisitors {
        let now = now_seconds();
        let today_index = day_index(now);
        let today = day_string(today_index);
        let window_start = today_index.saturating_sub(29);
        let runtime = self.visitors.lock().await;

        let mut visitors = runtime
            .disk
            .visitors
            .iter()
            .map(|(ip, record)| {
                let geo = record.geo.clone().unwrap_or_default();
                PublicVisitor {
                    ip: ip.clone(),
                    country: geo.country,
                    country_code: geo.country_code,
                    city: geo.city,
                    isp: geo.isp,
                    asn: geo.asn,
                    today_visits: record.days.get(&today).copied().unwrap_or_default(),
                    last_30_days_visits: window_visits(record, window_start),
                    total_visits: record.total_visits,
                    first_seen: record.first_seen,
                    last_seen: record.last_seen,
                    online: now.saturating_sub(record.last_seen) <= ONLINE_WINDOW_SECONDS,
                }
            })
            .collect::<Vec<_>>();
        visitors.sort_by(|left, right| {
            right
                .last_seen
                .cmp(&left.last_seen)
                .then_with(|| left.ip.cmp(&right.ip))
        });

        let known_visitors = visitors.len() as u64;
        let online_now = visitors.iter().filter(|visitor| visitor.online).count() as u64;
        visitors.truncate(MAX_PUBLIC_ROWS);

        PublicVisitors {
            generated_at: now,
            online_window_seconds: ONLINE_WINDOW_SECONDS,
            session_timeout_seconds: SESSION_TIMEOUT_SECONDS,
            known_visitors,
            listed_visitors: visitors.len() as u64,
            online_now,
            visitors,
        }
    }

    pub(crate) async fn visitor_ips_missing_geo(&self, limit: usize) -> Vec<IpAddr> {
        let now = now_seconds();
        let runtime = self.visitors.lock().await;
        runtime
            .disk
            .visitors
            .iter()
            .filter(|(_, record)| {
                record
                    .geo
                    .as_ref()
                    .is_none_or(|geo| now.saturating_sub(geo.updated_at) >= GEO_TTL_SECONDS)
            })
            .filter_map(|(ip, _)| ip.parse::<IpAddr>().ok())
            .take(limit)
            .collect()
    }

    pub(crate) async fn apply_visitor_geo(&self, locations: BTreeMap<IpAddr, VisitorGeo>) {
        if locations.is_empty() {
            return;
        }

        let snapshot = {
            let mut runtime = self.visitors.lock().await;
            for (ip, geo) in locations {
                if let Some(record) = runtime.disk.visitors.get_mut(&ip.to_string()) {
                    record.geo = Some(geo);
                }
            }
            runtime.last_saved = now_seconds();
            runtime.disk.clone()
        };

        self.save_visitors(&snapshot);
    }

    fn save_visitors(&self, disk: &VisitorsDisk) {
        if let Err(error) = save_visitors_disk(&self.visitors_path, disk) {
            warn!(
                path = %self.visitors_path.display(),
                error = ?error,
                "failed to persist visitor store"
            );
        }
    }
}

fn window_visits(record: &VisitorRecord, window_start: i64) -> u64 {
    record
        .days
        .iter()
        .filter(|(day, _)| parse_day_index(day).is_some_and(|index| index >= window_start))
        .map(|(_, visits)| *visits)
        .fold(0u64, |total, visits| total.saturating_add(visits))
}

fn prune_visitors(disk: &mut VisitorsDisk, today_index: i64) {
    let day_floor = today_index.saturating_sub(DAY_RETENTION_DAYS);
    for record in disk.visitors.values_mut() {
        record
            .days
            .retain(|day, _| parse_day_index(day).is_some_and(|index| index > day_floor));
    }

    let record_floor = today_index
        .saturating_sub(RECORD_RETENTION_DAYS)
        .saturating_mul(SECONDS_PER_DAY as i64)
        .max(0) as u64;
    disk.visitors
        .retain(|_, record| record.last_seen >= record_floor);

    if disk.visitors.len() > MAX_VISITOR_RECORDS {
        let mut by_last_seen = disk
            .visitors
            .iter()
            .map(|(ip, record)| (record.last_seen, ip.clone()))
            .collect::<Vec<_>>();
        by_last_seen.sort_unstable();
        let drop_count = disk.visitors.len() - MAX_VISITOR_RECORDS;
        for (_, ip) in by_last_seen.into_iter().take(drop_count) {
            disk.visitors.remove(&ip);
        }
    }
}

fn load_visitors_disk(path: &Path) -> Result<VisitorsDisk> {
    match fs::read_to_string(path) {
        Ok(content) => serde_json::from_str(&content)
            .with_context(|| format!("failed to parse {}", path.display())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(VisitorsDisk::default()),
        Err(error) => Err(error).with_context(|| format!("failed to read {}", path.display())),
    }
}

fn save_visitors_disk(path: &Path, disk: &VisitorsDisk) -> Result<()> {
    let content = serde_json::to_vec_pretty(disk)?;
    fsutil::write_file_atomic(path, &content, 0o600)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record_with_days(days: &[(&str, u64)], last_seen: u64) -> VisitorRecord {
        VisitorRecord {
            first_seen: 1,
            last_seen,
            total_visits: days.iter().map(|(_, visits)| visits).sum(),
            days: days
                .iter()
                .map(|(day, visits)| ((*day).to_owned(), *visits))
                .collect(),
            geo: None,
        }
    }

    #[test]
    fn window_visits_counts_only_days_inside_the_window() {
        let today_index = parse_day_index("2026-08-31").unwrap();
        let record = record_with_days(
            &[("2026-08-31", 3), ("2026-08-10", 5), ("2026-06-01", 40)],
            0,
        );

        assert_eq!(window_visits(&record, today_index.saturating_sub(29)), 8);
    }

    #[test]
    fn prune_drops_old_days_and_stale_records() {
        let today_index = parse_day_index("2026-08-31").unwrap();
        let now = (today_index as u64) * SECONDS_PER_DAY;
        let mut disk = VisitorsDisk::default();
        disk.visitors.insert(
            "203.0.113.5".to_owned(),
            record_with_days(&[("2026-08-31", 2), ("2026-01-01", 9)], now),
        );
        disk.visitors.insert(
            "198.51.100.9".to_owned(),
            record_with_days(&[("2024-01-01", 4)], 1),
        );

        prune_visitors(&mut disk, today_index);

        assert_eq!(disk.visitors.len(), 1);
        let kept = disk.visitors.get("203.0.113.5").unwrap();
        assert_eq!(kept.days.len(), 1);
        assert!(kept.days.contains_key("2026-08-31"));
    }

    #[test]
    fn prune_caps_the_number_of_stored_records() {
        let today_index = parse_day_index("2026-08-31").unwrap();
        let now = (today_index as u64) * SECONDS_PER_DAY;
        let mut disk = VisitorsDisk::default();
        for index in 0..(MAX_VISITOR_RECORDS + 10) {
            disk.visitors.insert(
                format!("10.0.{}.{}", index / 256, index % 256),
                record_with_days(&[("2026-08-31", 1)], now.saturating_sub(index as u64)),
            );
        }

        prune_visitors(&mut disk, today_index);

        assert_eq!(disk.visitors.len(), MAX_VISITOR_RECORDS);
        assert!(disk.visitors.contains_key("10.0.0.0"));
    }
}
