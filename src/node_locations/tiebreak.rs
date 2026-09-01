//! When ip-api and ipinfo disagree about a node's country, a third source
//! settles it. Without this, the node waits in the manual review queue and
//! never reaches the map.

use super::fields::{normalized_code, unknown_if_empty};
use super::geo_cache::{CachedGeoLocation, GeoCache};
use crate::config::NodeLocationsConfig;
use crate::geoip::trimmed_non_empty;
use serde::{Deserialize, Serialize};
use std::net::IpAddr;
use std::time::Duration;
use tokio::task::JoinSet;
use tracing::{info, warn};

const LOOKUP_CONCURRENCY: usize = 4;
const LOOKUP_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct TiebreakLocation {
    #[serde(default)]
    pub(super) country: Option<String>,
    #[serde(default)]
    pub(super) country_code: Option<String>,
    #[serde(default)]
    pub(super) city: Option<String>,
    #[serde(default)]
    pub(super) isp: Option<String>,
    #[serde(default)]
    pub(super) lat: Option<f64>,
    #[serde(default)]
    pub(super) lon: Option<f64>,
    #[serde(default)]
    pub(super) updated_at: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ConflictDecision {
    /// The third source backs ip-api, whose location is already stored.
    KeepIpApi,
    /// The third source backs ipinfo, and carries the city and coordinates
    /// that ipinfo lite does not have.
    AdoptTiebreak,
    /// Three answers, three countries: a person should look at this one.
    Unresolved,
}

pub(super) fn decide(
    ip_api_code: Option<&str>,
    ipinfo_code: Option<&str>,
    tiebreak_code: Option<&str>,
) -> ConflictDecision {
    let Some(tiebreak_code) = tiebreak_code else {
        return ConflictDecision::Unresolved;
    };
    if ip_api_code == Some(tiebreak_code) {
        return ConflictDecision::KeepIpApi;
    }
    if ipinfo_code == Some(tiebreak_code) {
        return ConflictDecision::AdoptTiebreak;
    }
    ConflictDecision::Unresolved
}

/// Settles what it can and reports how many conflicts it cleared.
pub(super) async fn resolve_conflicts(
    config: &NodeLocationsConfig,
    ips: &[IpAddr],
    geo_cache: &mut GeoCache,
    now: u64,
    ttl: Duration,
) -> usize {
    if !config.auto_resolve_conflicts {
        return 0;
    }

    let pending = ips
        .iter()
        .copied()
        .filter(|ip| {
            geo_cache.location(*ip).is_some_and(|location| {
                location.ipinfo_conflict && !location.has_fresh_tiebreak(now, ttl)
            })
        })
        .collect::<Vec<_>>();

    if !pending.is_empty() {
        for (ip, location) in lookup_locations(&config.tiebreak_base_url, &pending, now).await {
            if let Some(cached) = geo_cache.location_mut(ip) {
                cached.tiebreak = Some(location);
            }
        }
    }

    let mut resolved = 0;
    for ip in ips {
        let Some(location) = geo_cache.location_mut(*ip) else {
            continue;
        };
        if !location.ipinfo_conflict {
            continue;
        }
        let decision = decide(
            normalized_code(&location.country_code).as_deref(),
            location
                .ipinfo
                .as_ref()
                .and_then(|ipinfo| normalized_code(&ipinfo.country_code))
                .as_deref(),
            location
                .tiebreak
                .as_ref()
                .and_then(|tiebreak| normalized_code(&tiebreak.country_code))
                .as_deref(),
        );

        match decision {
            ConflictDecision::KeepIpApi => {
                info!(ip = %ip, country = %location.country, "conflict settled in favour of ip-api");
                location.clear_conflict();
                resolved += 1;
            }
            ConflictDecision::AdoptTiebreak => {
                let adopted = location.adopt_tiebreak();
                if adopted {
                    info!(ip = %ip, country = %location.country, "conflict settled in favour of ipinfo");
                    resolved += 1;
                }
            }
            ConflictDecision::Unresolved => {}
        }
    }

    resolved
}

async fn lookup_locations(
    base_url: &str,
    ips: &[IpAddr],
    now: u64,
) -> Vec<(IpAddr, TiebreakLocation)> {
    let http = crate::http::shared_client();
    let mut located = Vec::new();

    for chunk in ips.chunks(LOOKUP_CONCURRENCY) {
        let mut lookups = JoinSet::new();
        for ip in chunk {
            let http = http.clone();
            let url = format!("{}/{ip}", base_url.trim_end_matches('/'));
            let ip = *ip;
            lookups.spawn(async move {
                let response = match http.get(&url).timeout(LOOKUP_TIMEOUT).send().await {
                    Ok(response) => response,
                    Err(error) => {
                        warn!(ip = %ip, error = ?error, "conflict tiebreak lookup failed");
                        return None;
                    }
                };
                if !response.status().is_success() {
                    warn!(ip = %ip, status = %response.status(), "conflict tiebreak lookup returned an error");
                    return None;
                }
                match response.json::<TiebreakResponse>().await {
                    Ok(raw) => raw.into_location(now).map(|location| (ip, location)),
                    Err(error) => {
                        warn!(ip = %ip, error = ?error, "failed to decode the conflict tiebreak response");
                        None
                    }
                }
            });
        }

        while let Some(result) = lookups.join_next().await {
            match result {
                Ok(Some(located_ip)) => located.push(located_ip),
                Ok(None) => {}
                Err(error) => warn!(error = ?error, "conflict tiebreak task failed"),
            }
        }
    }

    located
}

#[derive(Debug, Deserialize)]
struct TiebreakResponse {
    #[serde(default)]
    success: Option<bool>,
    #[serde(default)]
    country: Option<String>,
    #[serde(default)]
    country_code: Option<String>,
    #[serde(default)]
    city: Option<String>,
    #[serde(default)]
    latitude: Option<f64>,
    #[serde(default)]
    longitude: Option<f64>,
    #[serde(default)]
    connection: Option<TiebreakConnection>,
}

#[derive(Debug, Deserialize)]
struct TiebreakConnection {
    #[serde(default)]
    isp: Option<String>,
    #[serde(default)]
    org: Option<String>,
}

impl TiebreakResponse {
    fn into_location(self, now: u64) -> Option<TiebreakLocation> {
        if self.success == Some(false) {
            return None;
        }
        let connection = self.connection.unwrap_or(TiebreakConnection {
            isp: None,
            org: None,
        });
        Some(TiebreakLocation {
            country: self.country.and_then(trimmed_non_empty),
            country_code: self.country_code.and_then(trimmed_non_empty),
            city: self.city.and_then(trimmed_non_empty),
            isp: connection
                .isp
                .and_then(trimmed_non_empty)
                .or_else(|| connection.org.and_then(trimmed_non_empty)),
            lat: self.latitude.filter(|value| value.is_finite()),
            lon: self.longitude.filter(|value| value.is_finite()),
            updated_at: now,
        })
    }
}

impl CachedGeoLocation {
    pub(super) fn has_fresh_tiebreak(&self, now: u64, ttl: Duration) -> bool {
        self.tiebreak
            .as_ref()
            .is_some_and(|tiebreak| now.saturating_sub(tiebreak.updated_at) < ttl.as_secs())
    }

    pub(super) fn clear_conflict(&mut self) {
        self.ipinfo_conflict = false;
        self.ipinfo_conflict_reason = None;
    }

    /// Takes the third source's location, which unlike ipinfo lite carries a
    /// city and coordinates.
    pub(super) fn adopt_tiebreak(&mut self) -> bool {
        let Some(tiebreak) = self.tiebreak.clone() else {
            return false;
        };
        let (Some(lat), Some(lon)) = (tiebreak.lat, tiebreak.lon) else {
            return false;
        };

        self.country = unknown_if_empty(tiebreak.country.as_deref().unwrap_or_default());
        self.country_code = tiebreak.country_code;
        self.city = unknown_if_empty(tiebreak.city.as_deref().unwrap_or_default());
        if let Some(isp) = tiebreak.isp {
            self.isp = isp;
        }
        self.lat = lat;
        self.lon = lon;
        self.source = "ipwho.is".to_owned();
        self.updated_at = tiebreak.updated_at;
        self.clear_conflict();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_third_answer_picks_the_side_it_agrees_with() {
        assert_eq!(
            decide(Some("US"), Some("NL"), Some("US")),
            ConflictDecision::KeepIpApi
        );
        assert_eq!(
            decide(Some("US"), Some("NL"), Some("NL")),
            ConflictDecision::AdoptTiebreak
        );
    }

    #[test]
    fn three_different_answers_stay_for_a_person() {
        assert_eq!(
            decide(Some("US"), Some("NL"), Some("DE")),
            ConflictDecision::Unresolved
        );
        assert_eq!(
            decide(Some("US"), Some("NL"), None),
            ConflictDecision::Unresolved
        );
    }

    #[test]
    fn a_failed_answer_is_not_a_location() {
        let raw = serde_json::json!({ "success": false, "message": "reserved range" });
        let response = serde_json::from_value::<TiebreakResponse>(raw).unwrap();

        assert!(response.into_location(42).is_none());
    }

    #[test]
    fn a_good_answer_carries_the_city_and_the_coordinates() {
        let raw = serde_json::json!({
            "success": true,
            "country": "Netherlands",
            "country_code": "NL",
            "city": "Amsterdam",
            "latitude": 52.37,
            "longitude": 4.89,
            "connection": { "isp": "ReliableSite.Net LLC", "org": "ReliableSite" },
        });
        let response = serde_json::from_value::<TiebreakResponse>(raw).unwrap();

        let location = response.into_location(7).unwrap();

        assert_eq!(location.country_code.as_deref(), Some("NL"));
        assert_eq!(location.city.as_deref(), Some("Amsterdam"));
        assert_eq!(location.isp.as_deref(), Some("ReliableSite.Net LLC"));
        assert_eq!(location.lat, Some(52.37));
        assert_eq!(location.updated_at, 7);
    }
}
