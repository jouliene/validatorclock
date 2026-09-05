use crate::geoip;
use crate::state::AppState;
use crate::state::visitors::VisitorGeo;
use crate::timeutil::now_sec;
use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use tracing::info;

const STARTUP_DELAY_SECONDS: u64 = 20;
const REFRESH_SECONDS: u64 = 120;
const BATCH_SIZE: usize = 100;

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

    let now = now_sec();
    let (private, public): (Vec<IpAddr>, Vec<IpAddr>) =
        pending.into_iter().partition(|ip| !is_public_ip(*ip));

    let mut locations = private
        .into_iter()
        .map(|ip| (ip, local_network_geo(now)))
        .collect::<BTreeMap<_, _>>();

    if !public.is_empty() {
        let endpoint = &state.config.node_locations.ip_api_batch_endpoint;
        locations.extend(
            geoip::lookup_batch(endpoint, &public)
                .await
                .into_iter()
                .map(|located| (located.ip, visitor_geo_from_lookup(located, now))),
        );
    }

    state.apply_visitor_geo(locations).await;
}

fn visitor_geo_from_lookup(located: geoip::IpApiLocation, now: u64) -> VisitorGeo {
    if !located.resolved {
        return VisitorGeo {
            country: Some("Unknown".to_owned()),
            updated_at: now,
            ..VisitorGeo::default()
        };
    }

    VisitorGeo {
        country: located.country,
        country_code: located.country_code,
        city: located.city,
        isp: located.isp,
        asn: located.asn,
        updated_at: now,
    }
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
    // A dual-stack client can arrive as ::ffff:a.b.c.d, which is only as
    // public as the IPv4 address inside it. Judged as a v6 address it looked
    // public whatever it wrapped, so a machine on a private network was sent
    // off for a geo lookup.
    if let Some(mapped) = ip.to_ipv4_mapped() {
        return is_public_ipv4(mapped);
    }

    let first = ip.segments()[0];
    !(ip.is_loopback()
        || ip.is_unspecified()
        || (first & 0xfe00) == 0xfc00
        || (first & 0xffc0) == 0xfe80)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A dual-stack client arrives as ::ffff:a.b.c.d. Judged as a v6 address
    /// it looked public whatever it wrapped, so a machine on a private network
    /// was sent off for a geo lookup.
    #[test]
    fn an_ipv4_address_inside_an_ipv6_one_is_judged_on_its_contents() {
        assert!(!is_public_ip("::ffff:192.168.1.10".parse().unwrap()));
        assert!(!is_public_ip("::ffff:127.0.0.1".parse().unwrap()));
        assert!(!is_public_ip("::ffff:10.0.0.7".parse().unwrap()));
        assert!(is_public_ip("::ffff:8.8.8.8".parse().unwrap()));
        // 203.0.113.0/24 is documentation, so it is not public inside a v6
        // address either.
        assert!(!is_public_ip("::ffff:203.0.113.9".parse().unwrap()));
        assert!(is_public_ip("2001:db8::1".parse().unwrap()));
    }

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
    fn a_failed_lookup_is_still_marked_as_resolved() {
        let located = geoip::IpApiLocation {
            ip: "203.0.113.9".parse().unwrap(),
            resolved: false,
            country: None,
            country_code: None,
            city: None,
            isp: None,
            asn: None,
            as_name: None,
            lat: None,
            lon: None,
        };

        let geo = visitor_geo_from_lookup(located, 42);

        assert_eq!(geo.country.as_deref(), Some("Unknown"));
        assert_eq!(geo.city, None);
        assert_eq!(geo.updated_at, 42);
    }
}
