//! Evidence and scheduling are separate from the selected map point.
use super::super::geo_cache::CachedGeoLocation;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const DAY: u64 = 86_400;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub(crate) struct ResearchConfig {
    pub enabled: bool,
    pub reuse_days: u64,
    pub max_ips_per_cycle: usize,
    pub daily_requests: u32,
    pub daily_measurements: u32,
    pub download_database: bool,
    pub operator_measurements: bool,
}
impl Default for ResearchConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            reuse_days: 90,
            max_ips_per_cycle: 100,
            daily_requests: 128,
            daily_measurements: 6,
            download_database: true,
            operator_measurements: true,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Observation {
    pub source: String,
    pub at: u64,
    pub city: String,
    pub country: String,
    pub country_code: String,
    pub region: String,
    pub lat: f64,
    pub lon: f64,
    pub asn: Option<String>,
    pub isp: String,
}
impl Observation {
    pub fn from_cached(value: &CachedGeoLocation) -> Self {
        Self {
            source: value.source.clone(),
            at: value.updated_at,
            city: value.city.clone(),
            country: value.country.clone(),
            country_code: value.country_code.clone().unwrap_or_default(),
            region: String::new(),
            lat: value.lat,
            lon: value.lon,
            asn: value.asn.clone(),
            isp: value.isp.clone(),
        }
    }
    pub fn valid(&self) -> bool {
        (-90.0..=90.0).contains(&self.lat)
            && (-180.0..=180.0).contains(&self.lon)
            && self.country_code.len() == 2
            && !self.country.is_empty()
    }
    pub fn cached(&self, confidence: &str) -> CachedGeoLocation {
        CachedGeoLocation {
            city: self.city.clone(),
            country: self.country.clone(),
            country_code: Some(self.country_code.clone()),
            isp: self.isp.clone(),
            asn: self.asn.clone(),
            as_name: Some(self.isp.clone()),
            lat: self.lat,
            lon: self.lon,
            source: self.source.clone(),
            confidence: confidence.into(),
            updated_at: self.at,
            ipinfo: None,
            ipinfo_checked_at: 0,
            ipinfo_conflict: false,
            ipinfo_conflict_settled: false,
            ipinfo_conflict_reason: None,
            tiebreak: None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Measurement {
    pub target: String,
    pub vantage: String,
    pub at: u64,
    pub min_ms: Option<f64>,
    pub output: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct OperatorEvidence {
    pub url: String,
    pub prefix: String,
    pub country_code: String,
    pub region: String,
    pub city: String,
    pub fetched_at: Option<u64>,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Entry {
    #[serde(default)]
    pub operator_evidence: Option<OperatorEvidence>,
    pub observations: BTreeMap<String, Observation>,
    pub measurements: Vec<Measurement>,
    pub decision: Option<Observation>,
    pub confidence: String,
    pub reasons: Vec<String>,
    pub next_attempt_at: u64,
    pub attempts: u32,
    pub last_seen_at: u64,
    pub completed_at: u64,
    pub generation_started_at: u64,
    pub secondary_checked: bool,
}
impl Entry {
    pub fn due(&self, now: u64, reuse_days: u64) -> bool {
        if self.completed_at > 0 {
            now >= self
                .completed_at
                .saturating_add(reuse_days.clamp(1, 3650) * DAY)
        } else {
            now >= self.next_attempt_at
        }
    }
    pub fn retry(&mut self, now: u64) {
        self.attempts = self.attempts.saturating_add(1);
        // Unavailable data: 1h, 6h, 1d, 3d, then once per week. Survives restarts.
        let delay = match self.attempts {
            1 => 3600,
            2 => 21600,
            3 => DAY,
            4 => 3 * DAY,
            _ => 7 * DAY,
        };
        self.next_attempt_at = now.saturating_add(delay);
    }
}
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Budget {
    pub day: u64,
    pub used: u32,
    pub measurements: u32,
    pub not_before: BTreeMap<String, u64>,
    pub total_requests: u64,
}
impl Budget {
    pub fn reserve(&mut self, source: &str, now: u64, config: &ResearchConfig) -> bool {
        if self.day < now / DAY {
            self.day = now / DAY;
            self.used = 0;
            self.measurements = 0;
        }
        if self.used >= config.daily_requests.min(500)
            || now < *self.not_before.get(source).unwrap_or(&0)
            || (source == "latitude-ping" && self.measurements >= config.daily_measurements.min(20))
        {
            return false;
        }
        self.used += 1;
        self.total_requests += 1;
        if source == "latitude-ping" {
            self.measurements += 1;
        }
        // Bounds requests across chains and cycles, without blocking the worker.
        self.not_before
            .insert(source.into(), now + if source == "ip-api" { 5 } else { 1 });
        true
    }
}
#[derive(Default, Deserialize, Serialize)]
pub struct Store {
    pub version: u32,
    pub entries: BTreeMap<String, Entry>,
    pub budget: Budget,
    pub assets: BTreeMap<String, u64>,
}

pub fn distance(a: &Observation, b: &Observation) -> f64 {
    let (la, lb) = (a.lat.to_radians(), b.lat.to_radians());
    let h = ((lb - la) / 2.0).sin().powi(2)
        + la.cos() * lb.cos() * ((b.lon - a.lon).to_radians() / 2.0).sin().powi(2);
    12742.0 * h.clamp(0.0, 1.0).sqrt().asin()
}

/// DB agreement is evidence, never physical verification. No majority voting.
pub fn decide(entry: &mut Entry) {
    entry.reasons.clear();
    let primary = entry.observations.get("ip-api").filter(|x| x.valid());
    let db = entry.observations.get("dbip").filter(|x| x.valid());
    let fallback = entry.observations.get("ipwho.is").filter(|x| x.valid());
    let Some(base) = primary.or(db).or(fallback).cloned() else {
        return;
    };
    for other in entry.observations.values().filter(|o| o.valid()) {
        if other.country_code != base.country_code {
            entry.reasons.push(format!(
                "country disagreement: {} / {}",
                base.source, other.source
            ));
        } else if distance(&base, other) > 100.0 {
            entry.reasons.push(format!(
                "coordinates differ by over 100 km: {} / {}",
                base.source, other.source
            ));
        }
    }
    entry.confidence = if entry.reasons.is_empty() {
        "approximate"
    } else {
        "disputed"
    }
    .into();
    entry.decision = Some(base);
}
