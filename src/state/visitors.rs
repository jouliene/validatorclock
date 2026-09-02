use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::net::IpAddr;
use std::path::Path;

use super::AppState;
use super::json_store::JsonStore;
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
    store: JsonStore<VisitorsDisk>,
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
    VisitorsRuntime {
        store: JsonStore::load(path.to_path_buf(), "visitor"),
    }
}

/// What one recorded event meant for the address that sent it. The aggregate
/// counters are built from these, so both readings of the traffic share one
/// notion of a visit and one notion of a visitor.
#[derive(Debug, Clone, Copy)]
pub(crate) struct VisitorEvent {
    pub(crate) starts_visit: bool,
    pub(crate) first_visit_today: bool,
    /// The moment this store filed the event. Analytics files it under the
    /// same one, so an event that crosses UTC midnight between the two cannot
    /// land on a different day than the visit it came from.
    pub(crate) recorded_at: u64,
}

impl AppState {
    pub(crate) async fn record_visitor(&self, ip: IpAddr) -> VisitorEvent {
        let now = now_seconds();
        let today_index = day_index(now);
        let today = day_string(today_index);

        let (event, snapshot) = {
            let mut runtime = self.visitors.lock().await;
            let record = runtime
                .store
                .get_mut()
                .visitors
                .entry(ip.to_string())
                .or_default();
            let first_visit_ever = record.first_seen == 0;
            let first_visit_today = !record.days.contains_key(&today);
            // A day boundary starts a visit even mid-session, so an address seen
            // today always shows up in today's numbers.
            let starts_visit = first_visit_ever
                || first_visit_today
                || now.saturating_sub(record.last_seen) > SESSION_TIMEOUT_SECONDS;
            let event = VisitorEvent {
                starts_visit,
                first_visit_today,
                recorded_at: now,
            };
            if first_visit_ever {
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
            let save_interval = if starts_visit {
                0
            } else {
                SAVE_INTERVAL_SECONDS
            };
            prune_visitors(runtime.store.get_mut(), today_index);
            (event, runtime.store.take_snapshot(now, save_interval))
        };

        if let Some(snapshot) = snapshot {
            snapshot.write().await;
        }
        event
    }

    /// How many distinct addresses were seen in each window, counted from the
    /// records themselves.
    ///
    /// The summary used to add up per-day counters instead, which counts an
    /// address once for every day it came back: four addresses over two days
    /// were reported as five visitors. A visitor is an address, in the table
    /// and in the summary alike, so both now count the same thing.
    pub(crate) async fn unique_visitors_for_windows(
        &self,
        today_index: i64,
        windows: [i64; 3],
    ) -> [u64; 3] {
        let runtime = self.visitors.lock().await;
        unique_visitors_in_windows(runtime.store.get(), today_index, windows)
    }

    /// Addresses seen within the online window, for the traffic summary.
    pub(crate) async fn visitors_online(&self) -> u64 {
        let now = now_seconds();
        let runtime = self.visitors.lock().await;
        runtime
            .store
            .get()
            .visitors
            .values()
            .filter(|record| now.saturating_sub(record.last_seen) <= ONLINE_WINDOW_SECONDS)
            .count() as u64
    }

    pub(crate) async fn public_visitors(&self) -> PublicVisitors {
        let now = now_seconds();
        let today_index = day_index(now);
        let today = day_string(today_index);
        let window_start = today_index.saturating_sub(29);
        let runtime = self.visitors.lock().await;

        let mut visitors = runtime
            .store
            .get()
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
            .store
            .get()
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
                if let Some(record) = runtime.store.get_mut().visitors.get_mut(&ip.to_string()) {
                    record.geo = Some(geo);
                }
            }
            runtime.store.take_snapshot(now_seconds(), 0)
        };

        if let Some(snapshot) = snapshot {
            snapshot.write().await;
        }
    }
}

/// One address is one visitor, however many days it came back on.
fn unique_visitors_in_windows(
    disk: &VisitorsDisk,
    today_index: i64,
    windows: [i64; 3],
) -> [u64; 3] {
    let mut counts = [0u64; 3];

    for record in disk.visitors.values() {
        let Some(latest) = record
            .days
            .keys()
            .filter_map(|day| parse_day_index(day))
            .max()
        else {
            continue;
        };
        for (count, days) in counts.iter_mut().zip(windows) {
            let first_day = today_index.saturating_sub(days.saturating_sub(1));
            if latest >= first_day {
                *count = count.saturating_add(1);
            }
        }
    }

    counts
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
    let record_floor = today_index
        .saturating_sub(RECORD_RETENTION_DAYS)
        .saturating_mul(SECONDS_PER_DAY as i64)
        .max(0) as u64;

    // A clock that has jumped forward - a restored snapshot, a dead RTC, one
    // bad NTP peer - puts these floors above everything on file. Pruning would
    // then empty the store in a single pass, and the empty store is what gets
    // written, so a wrong clock for one moment costs every visitor on record.
    // Retention never has a reason to remove everything at once, so a floor
    // that would is read as a wrong clock rather than obeyed.
    let would_empty_the_store = !disk.visitors.is_empty()
        && disk
            .visitors
            .values()
            .all(|record| record.last_seen < record_floor);
    if would_empty_the_store {
        return;
    }

    for record in disk.visitors.values_mut() {
        record
            .days
            .retain(|day, _| parse_day_index(day).is_some_and(|index| index > day_floor));
    }

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

    /// The summary used to add up per-day counters, so an address that came
    /// back on a second day was reported as two visitors: four addresses read
    /// as five. The table counts addresses, and so must the summary.
    #[test]
    fn an_address_that_comes_back_is_still_one_visitor() {
        let today = parse_day_index("2026-09-02").unwrap();
        let mut disk = VisitorsDisk::default();
        disk.visitors.insert(
            "203.0.113.1".to_owned(),
            record_with_days(&[("2026-09-01", 1), ("2026-09-02", 1)], 0),
        );
        disk.visitors.insert(
            "203.0.113.2".to_owned(),
            record_with_days(&[("2026-09-02", 1)], 0),
        );
        disk.visitors.insert(
            "203.0.113.3".to_owned(),
            record_with_days(&[("2026-08-20", 1)], 0),
        );

        let [today_count, week, month] = unique_visitors_in_windows(&disk, today, [1, 7, 30]);

        assert_eq!(today_count, 2);
        assert_eq!(
            week, 2,
            "the address that came back on two days is one visitor, not two"
        );
        assert_eq!(month, 3, "the older address is still inside 30 days");
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

    /// A clock that reads years ahead puts the retention floor above every
    /// record, and the emptied store is written straight to disk - so one bad
    /// clock reading costs all the visitor history there is, past recovery.
    #[test]
    fn a_clock_that_jumped_forward_does_not_empty_the_store() {
        let today_index = parse_day_index("2026-08-31").unwrap();
        let now = (today_index as u64) * SECONDS_PER_DAY;
        let mut disk = VisitorsDisk::default();
        disk.visitors.insert(
            "203.0.113.5".to_owned(),
            record_with_days(&[("2026-08-31", 2)], now),
        );
        disk.visitors.insert(
            "198.51.100.9".to_owned(),
            record_with_days(&[("2026-08-30", 1)], now - SECONDS_PER_DAY),
        );

        // Nine years ahead: a restored snapshot, or one bad NTP peer.
        let jumped = parse_day_index("2035-06-12").unwrap();
        prune_visitors(&mut disk, jumped);

        assert_eq!(disk.visitors.len(), 2, "nothing should have been dropped");
        assert!(
            disk.visitors["203.0.113.5"].days.contains_key("2026-08-31"),
            "the day counts should survive too"
        );
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
