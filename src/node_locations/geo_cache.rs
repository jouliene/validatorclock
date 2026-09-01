//! Cached ip-api locations for node addresses.

use super::fields::{
    ip_api_source, is_false, medium_confidence, normalized_code, normalized_name, number_field,
    number_u64_field, string_field, unknown_string,
};
use super::ipinfo::IpInfoLiteLocation;
use super::tiebreak::TiebreakLocation;
use crate::fsutil::write_file_atomic;
use crate::geoip;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::net::IpAddr;
use std::path::Path;
use std::time::Duration;

pub(super) async fn lookup_ip_api_locations(
    endpoint: &str,
    ips: &[IpAddr],
    now: u64,
) -> BTreeMap<IpAddr, CachedGeoLocation> {
    geoip::lookup_batch(endpoint, ips)
        .await
        .into_iter()
        .filter_map(|located| Some((located.ip, CachedGeoLocation::from_lookup(located, now)?)))
        .collect()
}
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub(super) struct GeoCache {
    #[serde(flatten)]
    pub(super) locations: BTreeMap<String, CachedGeoLocation>,
}

impl GeoCache {
    pub(super) fn location(&self, ip: IpAddr) -> Option<&CachedGeoLocation> {
        self.locations.get(&ip.to_string())
    }

    pub(super) fn location_mut(&mut self, ip: IpAddr) -> Option<&mut CachedGeoLocation> {
        self.locations.get_mut(&ip.to_string())
    }

    pub(super) fn has_fresh_location(&self, ip: IpAddr, now: u64, ttl: Duration) -> bool {
        self.location(ip)
            .is_some_and(|location| location.has_coordinates() && location.is_fresh(now, ttl))
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct CachedGeoLocation {
    #[serde(default = "unknown_string")]
    pub(super) city: String,
    #[serde(default = "unknown_string")]
    pub(super) country: String,
    #[serde(default)]
    pub(super) country_code: Option<String>,
    #[serde(default = "unknown_string")]
    pub(super) isp: String,
    #[serde(default)]
    pub(super) asn: Option<String>,
    #[serde(default)]
    pub(super) as_name: Option<String>,
    pub(super) lat: f64,
    pub(super) lon: f64,
    #[serde(default = "ip_api_source")]
    pub(super) source: String,
    #[serde(default = "medium_confidence")]
    pub(super) confidence: String,
    #[serde(default)]
    pub(super) updated_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) ipinfo: Option<IpInfoLiteLocation>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub(super) ipinfo_conflict: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) ipinfo_conflict_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) tiebreak: Option<TiebreakLocation>,
}

impl CachedGeoLocation {
    pub(super) fn from_lookup(located: geoip::IpApiLocation, now: u64) -> Option<Self> {
        if !located.resolved {
            return None;
        }
        Some(Self {
            city: located.city.unwrap_or_else(unknown_string),
            country: located.country.unwrap_or_else(unknown_string),
            country_code: located.country_code,
            isp: located.isp.unwrap_or_else(unknown_string),
            asn: located.asn,
            as_name: located.as_name,
            lat: located.lat?,
            lon: located.lon?,
            source: ip_api_source(),
            confidence: medium_confidence(),
            updated_at: now,
            ipinfo: None,
            ipinfo_conflict: false,
            ipinfo_conflict_reason: None,
            tiebreak: None,
        })
    }

    pub(super) fn has_coordinates(&self) -> bool {
        self.lat.is_finite() && self.lon.is_finite()
    }

    pub(super) fn is_fresh(&self, now: u64, ttl: Duration) -> bool {
        now.saturating_sub(self.updated_at) < ttl.as_secs()
    }

    pub(super) fn has_fresh_ipinfo(&self, now: u64, ttl: Duration) -> bool {
        self.ipinfo
            .as_ref()
            .is_some_and(|ipinfo| now.saturating_sub(ipinfo.updated_at) < ttl.as_secs())
    }

    pub(super) fn ipinfo_conflict_reason(&self) -> Option<String> {
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

pub(super) fn load_geo_cache(path: &Path) -> Result<GeoCache> {
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

pub(super) fn migrate_versioned_geo_cache(value: Value) -> GeoCache {
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
                tiebreak: None,
            },
        );
    }
    GeoCache { locations }
}

pub(super) fn save_geo_cache(path: &Path, cache: &GeoCache) -> Result<()> {
    let data =
        serde_json::to_vec_pretty(&cache.locations).context("failed to serialize geo cache")?;
    write_file_atomic(path, &data, 0o600)
}
