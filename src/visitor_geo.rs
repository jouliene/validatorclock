use crate::state::AppState;
use crate::state::visitors::VisitorGeo;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::time::sleep;
use tracing::{info, warn};

const STARTUP_DELAY_SECONDS: u64 = 20;
const REFRESH_SECONDS: u64 = 120;
const BATCH_SIZE: usize = 100;
const REQUEST_TIMEOUT_SECONDS: u64 = 20;

pub(crate) fn spawn_background_refresh(state: Arc<AppState>) {
    tokio::spawn(async move {
        background_refresh_loop(state).await;
    });
}

async fn background_refresh_loop(state: Arc<AppState>) {
    info!(
        refresh_seconds = REFRESH_SECONDS,
        "visitor geo background refresh started"
    );
    sleep(Duration::from_secs(STARTUP_DELAY_SECONDS)).await;

    loop {
        refresh_pending_visitor_geo(&state).await;
        sleep(Duration::from_secs(REFRESH_SECONDS)).await;
    }
}

async fn refresh_pending_visitor_geo(state: &AppState) {
    let pending = state.visitor_ips_missing_geo(BATCH_SIZE).await;
    if pending.is_empty() {
        return;
    }

    let now = now_seconds();
    let (private, public): (Vec<IpAddr>, Vec<IpAddr>) =
        pending.into_iter().partition(|ip| !is_public_ip(*ip));

    let mut locations = private
        .into_iter()
        .map(|ip| (ip, local_network_geo(now)))
        .collect::<BTreeMap<_, _>>();

    if !public.is_empty() {
        let http = match reqwest::Client::builder()
            .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECONDS))
            .build()
        {
            Ok(http) => http,
            Err(error) => {
                warn!(error = ?error, "failed to build visitor geo HTTP client");
                return;
            }
        };
        let endpoint = &state.config.node_locations.ip_api_batch_endpoint;
        locations.extend(lookup_ip_api_locations(&http, endpoint, &public, now).await);
    }

    state.apply_visitor_geo(locations).await;
}

async fn lookup_ip_api_locations(
    http: &reqwest::Client,
    endpoint: &str,
    ips: &[IpAddr],
    now: u64,
) -> BTreeMap<IpAddr, VisitorGeo> {
    let mut output = BTreeMap::new();
    for chunk in ips.chunks(BATCH_SIZE) {
        let requests = chunk.iter().map(IpAddr::to_string).collect::<Vec<_>>();
        let response = match http.post(endpoint).json(&requests).send().await {
            Ok(response) => response,
            Err(error) => {
                warn!(error = ?error, "visitor geo batch lookup failed");
                continue;
            }
        };
        if !response.status().is_success() {
            warn!(status = %response.status(), "visitor geo batch lookup returned an error");
            continue;
        }
        let raw = match response.json::<Vec<IpApiVisitorResponse>>().await {
            Ok(raw) => raw,
            Err(error) => {
                warn!(error = ?error, "failed to decode visitor geo batch response");
                continue;
            }
        };
        output.extend(raw.into_iter().filter_map(|item| item.into_geo(now)));
    }
    output
}

fn local_network_geo(now: u64) -> VisitorGeo {
    VisitorGeo {
        country: Some("Local network".to_owned()),
        country_code: None,
        city: None,
        isp: None,
        asn: None,
        updated_at: now,
    }
}

fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => is_public_ipv4(ip),
        IpAddr::V6(ip) => is_public_ipv6(ip),
    }
}

fn is_public_ipv4(ip: Ipv4Addr) -> bool {
    !(ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_broadcast()
        || ip.is_documentation()
        || ip.is_unspecified()
        || ip.octets()[0] == 0
        || (ip.octets()[0] == 100 && (64..128).contains(&ip.octets()[1]))
        || ip.octets()[0] >= 240)
}

fn is_public_ipv6(ip: Ipv6Addr) -> bool {
    let first = ip.segments()[0];
    !(ip.is_loopback()
        || ip.is_unspecified()
        || (first & 0xfe00) == 0xfc00
        || (first & 0xffc0) == 0xfe80)
}

fn now_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[derive(Debug, Deserialize)]
struct IpApiVisitorResponse {
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
    isp: Option<String>,
    #[serde(default, rename = "as")]
    as_text: Option<String>,
}

impl IpApiVisitorResponse {
    fn into_geo(self, now: u64) -> Option<(IpAddr, VisitorGeo)> {
        let ip = self.query?.parse::<IpAddr>().ok()?;
        if self.status.as_deref() != Some("success") {
            return Some((
                ip,
                VisitorGeo {
                    country: Some("Unknown".to_owned()),
                    updated_at: now,
                    ..VisitorGeo::default()
                },
            ));
        }
        Some((
            ip,
            VisitorGeo {
                country: self.country.and_then(trimmed_non_empty),
                country_code: self.country_code.and_then(trimmed_non_empty),
                city: self.city.and_then(trimmed_non_empty),
                isp: self.isp.and_then(trimmed_non_empty),
                asn: self.as_text.as_deref().and_then(parse_asn),
                updated_at: now,
            },
        ))
    }
}

fn trimmed_non_empty(value: String) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

fn parse_asn(value: &str) -> Option<String> {
    let token = value.split_whitespace().next()?;
    token
        .strip_prefix("AS")
        .filter(|digits| !digits.is_empty() && digits.chars().all(|ch| ch.is_ascii_digit()))
        .map(|_| token.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_and_loopback_addresses_are_not_public() {
        assert!(!is_public_ip("127.0.0.1".parse().unwrap()));
        assert!(!is_public_ip("192.168.1.10".parse().unwrap()));
        assert!(!is_public_ip("10.4.0.7".parse().unwrap()));
        assert!(!is_public_ip("100.64.3.9".parse().unwrap()));
        assert!(!is_public_ip("::1".parse().unwrap()));
        assert!(!is_public_ip("fe80::1".parse().unwrap()));
        assert!(!is_public_ip("203.0.113.9".parse().unwrap()));
        assert!(is_public_ip("8.8.8.8".parse().unwrap()));
        assert!(is_public_ip("2606:4700:4700::1111".parse().unwrap()));
    }

    #[test]
    fn ip_api_success_maps_to_visitor_geo() {
        let raw = serde_json::json!({
            "status": "success",
            "query": "203.0.113.9",
            "country": "United States",
            "countryCode": "US",
            "city": "San Francisco",
            "isp": "OVH SAS",
            "as": "AS16276 OVH SAS",
        });
        let response = serde_json::from_value::<IpApiVisitorResponse>(raw).unwrap();

        let (ip, geo) = response.into_geo(1_700_000_000).unwrap();

        assert_eq!(ip, "203.0.113.9".parse::<IpAddr>().unwrap());
        assert_eq!(geo.country.as_deref(), Some("United States"));
        assert_eq!(geo.city.as_deref(), Some("San Francisco"));
        assert_eq!(geo.isp.as_deref(), Some("OVH SAS"));
        assert_eq!(geo.asn.as_deref(), Some("AS16276"));
        assert_eq!(geo.updated_at, 1_700_000_000);
    }

    #[test]
    fn ip_api_failure_still_marks_the_address_as_resolved() {
        let raw = serde_json::json!({
            "status": "fail",
            "query": "203.0.113.9",
            "message": "reserved range",
        });
        let response = serde_json::from_value::<IpApiVisitorResponse>(raw).unwrap();

        let (_, geo) = response.into_geo(42).unwrap();

        assert_eq!(geo.country.as_deref(), Some("Unknown"));
        assert_eq!(geo.city, None);
    }
}
