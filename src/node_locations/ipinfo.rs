//! ipinfo lookups that double-check the ip-api answer for a node address.

use super::fields::{unknown_if_empty, unknown_string};
use super::geo_cache::GeoCache;
use super::manual_review::ManualResolvedIp;
use crate::config::NodeLocationsConfig;
use crate::geoip::trimmed_non_empty;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::net::IpAddr;
use std::time::Duration;
use tokio::task::JoinSet;
use tracing::warn;

pub(super) const IPINFO_CONCURRENCY: usize = 16;
pub(super) const IPINFO_LOOKUP_TIMEOUT: Duration = Duration::from_secs(20);
/// ipinfo answers one address per request, so an address it cannot place used
/// to cost a request on every refresh cycle, for as long as the node was up.
/// A failure waits this long before it is worth asking again.
const IPINFO_RETRY_AFTER: Duration = Duration::from_secs(6 * 60 * 60);

pub(super) async fn refresh_ipinfo_verification(
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
            geo_cache.location(*ip).is_some_and(|location| {
                !location.has_fresh_ipinfo(now, ttl)
                    && !location.ipinfo_asked_recently(now, IPINFO_RETRY_AFTER)
            })
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
            // A fresh answer deserves a fresh look, even where an older
            // disagreement had already been settled.
            location.ipinfo_conflict_settled = false;
        }
    }
    // Every address that was asked is marked, answered or not, so silence is
    // not paid for again on the next cycle.
    for ip in &lookup_ips {
        if let Some(location) = geo_cache.location_mut(*ip) {
            location.ipinfo_checked_at = now;
        }
    }
    lookup_ips.len()
}

pub(super) async fn lookup_ipinfo_lite_locations(
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

pub(super) async fn lookup_ipinfo_lite_one(
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

    let response = match http.get(url).timeout(IPINFO_LOOKUP_TIMEOUT).send().await {
        Ok(response) => response,
        Err(error) => {
            // The token rides in the query string and a reqwest error prints
            // the URL it failed on, so the URL is dropped before logging.
            warn!(ip = %ip, error = ?error.without_url(), "ipinfo lookup failed");
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
            warn!(ip = %ip, error = ?error.without_url(), "failed to decode ipinfo response");
            return None;
        }
    };
    raw.into_location(ip, now).map(|location| (ip, location))
}

pub(super) fn refresh_ipinfo_conflicts(ips: &[IpAddr], geo_cache: &mut GeoCache) -> bool {
    let mut changed = false;
    for ip in ips {
        let Some(location) = geo_cache.location_mut(*ip) else {
            continue;
        };
        if location.ipinfo_conflict_settled {
            // A third source already decided this one. Finding the same
            // disagreement again every cycle only rewrote the cache file.
            continue;
        }
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
#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct IpInfoLiteLocation {
    #[serde(default)]
    pub(super) asn: Option<String>,
    #[serde(default)]
    pub(super) as_name: Option<String>,
    #[serde(default)]
    pub(super) as_domain: Option<String>,
    #[serde(default)]
    pub(super) country_code: Option<String>,
    #[serde(default = "unknown_string")]
    pub(super) country: String,
    #[serde(default)]
    pub(super) continent_code: Option<String>,
    #[serde(default)]
    pub(super) continent: Option<String>,
    #[serde(default)]
    pub(super) updated_at: u64,
}

#[derive(Debug, Deserialize)]
pub(super) struct IpInfoLiteResponse {
    #[serde(default)]
    pub(super) ip: Option<String>,
    #[serde(default)]
    pub(super) asn: Option<String>,
    #[serde(default)]
    pub(super) as_name: Option<String>,
    #[serde(default)]
    pub(super) as_domain: Option<String>,
    #[serde(default)]
    pub(super) country_code: Option<String>,
    #[serde(default)]
    pub(super) country: Option<String>,
    #[serde(default)]
    pub(super) continent_code: Option<String>,
    #[serde(default)]
    pub(super) continent: Option<String>,
}

impl IpInfoLiteResponse {
    pub(super) fn into_location(
        self,
        requested_ip: IpAddr,
        now: u64,
    ) -> Option<IpInfoLiteLocation> {
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
