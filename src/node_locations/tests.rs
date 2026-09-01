use super::candidates::*;
use super::fields::*;
use super::geo_cache::*;
use super::ipinfo::*;
use super::manual_review::*;
use super::map_nodes::*;
use serde_json::json;
use std::collections::BTreeMap;

fn cached_location(city: &str, country: &str, country_code: &str) -> CachedGeoLocation {
    CachedGeoLocation {
        city: city.to_owned(),
        country: country.to_owned(),
        country_code: Some(country_code.to_owned()),
        isp: "Test ISP".to_owned(),
        asn: Some("AS64500".to_owned()),
        as_name: Some("Test ISP".to_owned()),
        lat: 1.25,
        lon: 2.5,
        source: ip_api_source(),
        confidence: medium_confidence(),
        updated_at: 1_700_000_000,
        ipinfo: None,
        ipinfo_conflict: false,
        ipinfo_conflict_reason: None,
        tiebreak: None,
    }
}

fn previous_map_node(peer: &str, ip: &str, last_seen_at: u64) -> MapNode {
    MapNode {
        peer: peer.to_owned(),
        ip: ip.to_owned(),
        city: "Previous City".to_owned(),
        country: "Previousland".to_owned(),
        isp: "Previous ISP".to_owned(),
        lat: 3.0,
        lon: 4.0,
        geo_source: ip_api_source(),
        geo_confidence: medium_confidence(),
        geo_updated_at: 1_700_000_000,
        last_seen_at,
    }
}

#[test]
fn parses_array_seed_records() {
    let candidates = collect_candidates_from_value(
        &json!([
            {"peer": "peer-a", "ip": "203.0.113.10:3030"},
            {"public_key": "peer-b", "addresses": ["[2001:db8::1]:3030", "198.51.100.9"]}
        ]),
        None,
    );

    assert_eq!(
        unique_candidates(candidates),
        vec![
            CandidateNode {
                peer: "peer-a".to_owned(),
                ip: "203.0.113.10".parse().unwrap(),
            },
            CandidateNode {
                peer: "peer-b".to_owned(),
                ip: "198.51.100.9".parse().unwrap(),
            },
            CandidateNode {
                peer: "peer-b".to_owned(),
                ip: "2001:db8::1".parse().unwrap(),
            },
        ]
    );
}

#[test]
fn parses_peer_keyed_seed_records() {
    let candidates = collect_candidates_from_value(
        &json!({
            "peer-a": "203.0.113.10",
            "peer-b": ["198.51.100.9:3030", "198.51.100.9:3030"]
        }),
        None,
    );

    assert_eq!(
        unique_candidates(candidates),
        vec![
            CandidateNode {
                peer: "peer-a".to_owned(),
                ip: "203.0.113.10".parse().unwrap(),
            },
            CandidateNode {
                peer: "peer-b".to_owned(),
                ip: "198.51.100.9".parse().unwrap(),
            },
        ]
    );
}

#[test]
fn parses_resolver_full_validator_records() {
    let candidates = collect_candidates_from_value(
        &json!({
            "validators": [
                {
                    "validator_public_key": "peer-a",
                    "resolution": {
                        "status": "resolved",
                        "addresses": [
                            {"ip": "203.0.113.10", "port": 30313, "version": "udp4"}
                        ]
                    }
                }
            ]
        }),
        None,
    );

    assert_eq!(
        unique_candidates(candidates),
        vec![CandidateNode {
            peer: "peer-a".to_owned(),
            ip: "203.0.113.10".parse().unwrap(),
        }]
    );
}

#[test]
fn builds_backward_compatible_map_nodes() {
    let candidates = vec![CandidateNode {
        peer: "peer-a".to_owned(),
        ip: "203.0.113.10".parse().unwrap(),
    }];
    let mut cache = GeoCache::default();
    cache.locations.insert(
        "203.0.113.10".to_owned(),
        cached_location("Test City", "Testland", "TL"),
    );

    let nodes = build_map_nodes_from_candidates(&candidates, &cache, &BTreeMap::new());
    let node = serde_json::to_value(&nodes[0]).unwrap();

    assert_eq!(node["peer"], "peer-a");
    assert_eq!(node["ip"], "203.0.113.10");
    assert_eq!(node["city"], "Test City");
    assert_eq!(node["country"], "Testland");
    assert_eq!(node["isp"], "Test ISP");
    assert_eq!(node["lat"], 1.25);
    assert_eq!(node["lon"], 2.5);
}

#[test]
fn manual_resolved_ip_overrides_cached_location() {
    let ip = "203.0.113.10".parse().unwrap();
    let candidates = vec![CandidateNode {
        peer: "peer-a".to_owned(),
        ip,
    }];
    let mut cache = GeoCache::default();
    cache.locations.insert(
        "203.0.113.10".to_owned(),
        cached_location("Wrong City", "Wrongland", "WL"),
    );
    let manual_resolved = BTreeMap::from([(
        ip,
        ManualResolvedIp {
            ip,
            geo: ManualGeo {
                city: "Manual City".to_owned(),
                country: "Manualia".to_owned(),
                latitude: 9.0,
                longitude: 10.0,
            },
            as_info: Some(ManualAs {
                name: "Manual ISP".to_owned(),
            }),
            updated_at: Some(1_800_000_000),
        },
    )]);

    let nodes = build_map_nodes_from_candidates(&candidates, &cache, &manual_resolved);
    let node = serde_json::to_value(&nodes[0]).unwrap();

    assert_eq!(node["city"], "Manual City");
    assert_eq!(node["country"], "Manualia");
    assert_eq!(node["isp"], "Manual ISP");
    assert_eq!(node["lat"], 9.0);
    assert_eq!(node["lon"], 10.0);
    assert_eq!(node["geo_source"], "manual");
    assert_eq!(node["geo_confidence"], "manual");
}

#[test]
fn ipinfo_country_conflict_holds_node_for_manual_review() {
    let ip = "203.0.113.10".parse().unwrap();
    let candidates = vec![CandidateNode {
        peer: "peer-a".to_owned(),
        ip,
    }];
    let mut cache = GeoCache::default();
    let mut location = cached_location("Test City", "United States", "US");
    location.ipinfo = Some(IpInfoLiteLocation {
        asn: Some("AS64500".to_owned()),
        as_name: Some("Test ISP".to_owned()),
        as_domain: Some("example.net".to_owned()),
        country_code: Some("BR".to_owned()),
        country: "Brazil".to_owned(),
        continent_code: Some("SA".to_owned()),
        continent: Some("South America".to_owned()),
        updated_at: 1_700_000_001,
    });
    cache.locations.insert("203.0.113.10".to_owned(), location);

    assert!(refresh_ipinfo_conflicts(&[ip], &mut cache));
    assert!(cache.location(ip).unwrap().ipinfo_conflict);

    let nodes = build_map_nodes_from_candidates(&candidates, &cache, &BTreeMap::new());
    assert!(nodes.is_empty());
}

#[test]
fn retains_previous_map_node_for_transient_missing_candidate() {
    let previous_nodes = PreviousMapNodes {
        nodes: vec![previous_map_node("peer-a", "203.0.113.10", 1_700_000_000)],
        updated_at: None,
    };

    let built = build_map_nodes_from_candidates_with_retention(
        &[],
        &GeoCache::default(),
        &BTreeMap::new(),
        &previous_nodes,
        1_700_000_300,
    );

    assert_eq!(built.retained_node_count, 1);
    assert_eq!(built.nodes.len(), 1);
    assert_eq!(built.nodes[0].peer, "peer-a");
}

#[test]
fn expires_previous_map_node_after_retention_window() {
    let previous_nodes = PreviousMapNodes {
        nodes: vec![previous_map_node("peer-a", "203.0.113.10", 1_700_000_000)],
        updated_at: None,
    };

    let built = build_map_nodes_from_candidates_with_retention(
        &[],
        &GeoCache::default(),
        &BTreeMap::new(),
        &previous_nodes,
        1_700_003_601,
    );

    assert_eq!(built.retained_node_count, 0);
    assert!(built.nodes.is_empty());
}

#[test]
fn retention_uses_file_timestamp_for_legacy_nodes_without_last_seen_at() {
    let legacy_node: MapNode = serde_json::from_value(json!({
        "peer": "peer-a",
        "ip": "203.0.113.10",
        "city": "Previous City",
        "country": "Previousland",
        "isp": "Previous ISP",
        "lat": 3.0,
        "lon": 4.0,
        "geo_source": ip_api_source(),
        "geo_confidence": medium_confidence(),
        "geo_updated_at": 1_700_000_000
    }))
    .unwrap();
    let previous_nodes = PreviousMapNodes {
        nodes: vec![legacy_node],
        updated_at: Some(1_700_000_000),
    };

    let built = build_map_nodes_from_candidates_with_retention(
        &[],
        &GeoCache::default(),
        &BTreeMap::new(),
        &previous_nodes,
        1_700_000_300,
    );

    assert_eq!(built.retained_node_count, 1);
    assert_eq!(built.nodes.len(), 1);
    assert_eq!(built.nodes[0].peer, "peer-a");
    assert_eq!(built.nodes[0].last_seen_at, 0);
}

#[test]
fn ipinfo_conflict_does_not_retain_previous_map_node_for_same_peer() {
    let ip = "203.0.113.10".parse().unwrap();
    let candidates = vec![CandidateNode {
        peer: "peer-a".to_owned(),
        ip,
    }];
    let mut cache = GeoCache::default();
    let mut location = cached_location("Test City", "United States", "US");
    location.ipinfo = Some(IpInfoLiteLocation {
        asn: Some("AS64500".to_owned()),
        as_name: Some("Test ISP".to_owned()),
        as_domain: Some("example.net".to_owned()),
        country_code: Some("BR".to_owned()),
        country: "Brazil".to_owned(),
        continent_code: Some("SA".to_owned()),
        continent: Some("South America".to_owned()),
        updated_at: 1_700_000_001,
    });
    cache.locations.insert("203.0.113.10".to_owned(), location);
    assert!(refresh_ipinfo_conflicts(&[ip], &mut cache));

    let previous_nodes = PreviousMapNodes {
        nodes: vec![previous_map_node("peer-a", "203.0.113.10", 1_700_000_000)],
        updated_at: None,
    };
    let built = build_map_nodes_from_candidates_with_retention(
        &candidates,
        &cache,
        &BTreeMap::new(),
        &previous_nodes,
        1_700_000_300,
    );

    assert_eq!(built.retained_node_count, 0);
    assert!(built.nodes.is_empty());
}

#[test]
fn netherlands_country_aliases_do_not_create_manual_review() {
    let ip = "203.0.113.10".parse().unwrap();
    let candidates = vec![CandidateNode {
        peer: "peer-a".to_owned(),
        ip,
    }];
    let mut cache = GeoCache::default();
    let mut location = cached_location("Amsterdam", "Netherland", "");
    location.country_code = None;
    location.ipinfo = Some(IpInfoLiteLocation {
        asn: Some("AS64500".to_owned()),
        as_name: Some("Test ISP".to_owned()),
        as_domain: Some("example.net".to_owned()),
        country_code: None,
        country: "The Netherlands".to_owned(),
        continent_code: Some("EU".to_owned()),
        continent: Some("Europe".to_owned()),
        updated_at: 1_700_000_001,
    });
    cache.locations.insert("203.0.113.10".to_owned(), location);

    assert!(!refresh_ipinfo_conflicts(&[ip], &mut cache));
    assert!(!cache.location(ip).unwrap().ipinfo_conflict);
    assert_eq!(
        normalized_name("The Netherlands"),
        normalized_name("Netherlands")
    );

    let nodes = build_map_nodes_from_candidates(&candidates, &cache, &BTreeMap::new());
    assert_eq!(nodes.len(), 1);
}

#[test]
fn manual_review_file_name_is_ipv6_safe() {
    assert_eq!(
        manual_ip_file_name("2804:388:425b:c8b:10d3:81b7:646c:9b32".parse().unwrap()),
        "2804_388_425b_c8b_10d3_81b7_646c_9b32.json"
    );
}
