use super::candidates::*;
use super::fields::*;
use super::geo_cache::*;
use super::ipinfo::*;
use super::manual_review::*;
use super::map_nodes::*;
use super::prune_geo_cache;
use super::tiebreak::TiebreakLocation;
use serde_json::json;
use std::collections::BTreeMap;
use std::net::IpAddr;
use std::time::Duration;

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
        ipinfo_checked_at: 0,
        ipinfo_conflict: false,
        ipinfo_conflict_settled: false,
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
                confirmed_at: None,
            },
            CandidateNode {
                peer: "peer-b".to_owned(),
                ip: "198.51.100.9".parse().unwrap(),
                confirmed_at: None,
            },
            CandidateNode {
                peer: "peer-b".to_owned(),
                ip: "2001:db8::1".parse().unwrap(),
                confirmed_at: None,
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
                confirmed_at: None,
            },
            CandidateNode {
                peer: "peer-b".to_owned(),
                ip: "198.51.100.9".parse().unwrap(),
                confirmed_at: None,
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
            confirmed_at: None,
        }]
    );
}

#[test]
fn builds_backward_compatible_map_nodes() {
    let candidates = vec![CandidateNode {
        peer: "peer-a".to_owned(),
        ip: "203.0.113.10".parse().unwrap(),
        confirmed_at: None,
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
        confirmed_at: None,
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
        confirmed_at: None,
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
        confirmed_at: None,
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
        confirmed_at: None,
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

/// The ipinfo token rides in the query string, and a reqwest error prints the
/// URL it failed on. Logging the error as it comes would put the token in the
/// log, so the URL is dropped first.
#[tokio::test]
async fn a_failed_ipinfo_lookup_is_logged_without_the_token() {
    const TOKEN: &str = "token-that-must-not-be-logged";

    // A port nothing listens on: the request is refused at once.
    let closed_port = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        port
    };

    let error = reqwest::Client::new()
        .get(format!(
            "http://127.0.0.1:{closed_port}/lite/203.0.113.1?token={TOKEN}"
        ))
        .timeout(std::time::Duration::from_secs(2))
        .send()
        .await
        .expect_err("a refused request should fail");

    assert!(
        format!("{error:?}").contains(TOKEN),
        "this test proves nothing unless the raw error carries the token"
    );
    assert!(
        !format!("{:?}", error.without_url()).contains(TOKEN),
        "the form that reaches the log must not carry the token"
    );
}

fn ipinfo_location(country: &str, country_code: &str) -> IpInfoLiteLocation {
    IpInfoLiteLocation {
        asn: Some("AS64500".to_owned()),
        as_name: Some("Test ISP".to_owned()),
        as_domain: Some("example.net".to_owned()),
        country_code: Some(country_code.to_owned()),
        country: country.to_owned(),
        continent_code: Some("EU".to_owned()),
        continent: Some("Europe".to_owned()),
        updated_at: 1_700_000_001,
    }
}

/// Two sources that agree on the ISO code agree. Calling the same country by
/// two names used to raise a conflict, and settling it spent a third lookup.
#[test]
fn agreeing_on_the_country_code_is_not_a_conflict() {
    let mut location = cached_location("Prague", "Czech Republic", "CZ");
    location.ipinfo = Some(ipinfo_location("Czechia", "CZ"));

    assert_eq!(location.ipinfo_conflict_reason(), None);

    location.ipinfo = Some(ipinfo_location("Netherlands", "NL"));
    assert!(
        location.ipinfo_conflict_reason().is_some(),
        "a real disagreement should still be reported"
    );
}

/// A settled disagreement stays settled: the sources still disagree, so it was
/// found and settled again on every refresh, rewriting the cache each time.
#[test]
fn a_settled_conflict_is_not_raised_again() {
    let ip: IpAddr = "203.0.113.11".parse().unwrap();
    let mut cache = GeoCache::default();
    let mut location = cached_location("Test City", "United States", "US");
    location.ipinfo = Some(ipinfo_location("Brazil", "BR"));
    cache.locations.insert(ip.to_string(), location);

    assert!(refresh_ipinfo_conflicts(&[ip], &mut cache));
    assert!(cache.location(ip).unwrap().ipinfo_conflict);

    cache.location_mut(ip).unwrap().clear_conflict();

    assert!(
        !refresh_ipinfo_conflicts(&[ip], &mut cache),
        "a settled conflict should not be raised again, nor rewrite the cache"
    );
    assert!(!cache.location(ip).unwrap().ipinfo_conflict);
}

/// ipinfo answers one address per request, so an address it could not place
/// used to cost a request on every cycle for as long as the node was up.
#[test]
fn an_address_ipinfo_could_not_place_is_not_asked_again_at_once() {
    let now = 1_700_000_000;
    let mut location = cached_location("Test City", "United States", "US");
    location.ipinfo_checked_at = now;

    assert!(location.ipinfo_asked_recently(now + 60, Duration::from_secs(6 * 60 * 60)));
    assert!(!location.ipinfo_asked_recently(now + 7 * 60 * 60, Duration::from_secs(6 * 60 * 60)));

    let never_asked = cached_location("Test City", "United States", "US");
    assert!(!never_asked.ipinfo_asked_recently(now, Duration::from_secs(6 * 60 * 60)));
}

/// A location off the globe would place a node nowhere and travel all the way
/// to the map.
#[test]
fn coordinates_outside_the_globe_are_not_a_location() {
    let off_globe = crate::geoip::IpApiLocation {
        ip: "203.0.113.12".parse().unwrap(),
        resolved: true,
        country: Some("Nowhere".to_owned()),
        country_code: Some("NW".to_owned()),
        city: Some("Nowhere".to_owned()),
        isp: Some("Test ISP".to_owned()),
        asn: None,
        as_name: None,
        lat: Some(120.0),
        lon: Some(4.9),
    };

    assert!(CachedGeoLocation::from_lookup(off_globe.clone(), 1).is_none());
    assert!(
        CachedGeoLocation::from_lookup(
            crate::geoip::IpApiLocation {
                lat: Some(52.37),
                ..off_globe
            },
            1
        )
        .is_some()
    );
}

/// A latitude off the globe reaches the public map and then throws inside
/// MapLibre when a visitor opens the cluster holding it, so the map stops
/// responding for everyone until the entry ages out.
#[test]
fn a_tiebreak_answer_off_the_globe_is_not_adopted() {
    let mut location = cached_location("Test City", "United States", "US");
    location.tiebreak = Some(TiebreakLocation {
        country: Some("Netherlands".to_owned()),
        country_code: Some("NL".to_owned()),
        city: Some("Amsterdam".to_owned()),
        isp: None,
        lat: Some(914.5),
        lon: Some(4.89),
        updated_at: 1,
    });

    assert!(
        !location.adopt_tiebreak(),
        "a latitude off the globe must not be adopted"
    );
    assert_eq!(location.lat, 1.25, "the original location should stand");

    let mut good = cached_location("Test City", "United States", "US");
    good.tiebreak = Some(TiebreakLocation {
        country: Some("Netherlands".to_owned()),
        country_code: Some("NL".to_owned()),
        city: Some("Amsterdam".to_owned()),
        isp: None,
        lat: Some(52.37),
        lon: Some(4.89),
        updated_at: 1,
    });
    assert!(
        good.adopt_tiebreak(),
        "a real location should still be taken"
    );
}

/// The cache only ever grew: nothing removed an entry, and every cycle read
/// and rewrote the whole file.
#[test]
fn the_geo_cache_drops_addresses_nobody_names_any_more() {
    let now = 1_700_000_000u64;
    let old = now - 60 * 24 * 60 * 60;
    let mut cache = GeoCache::default();
    for (ip, updated_at) in [
        ("203.0.113.1", now), // still a candidate
        ("203.0.113.2", old), // gone, and stale
        ("203.0.113.3", now), // gone, but refreshed recently
    ] {
        let mut location = cached_location("Test City", "United States", "US");
        location.updated_at = updated_at;
        cache.locations.insert(ip.to_owned(), location);
    }
    let seen = ["203.0.113.1".parse::<IpAddr>().unwrap()]
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>();

    assert!(prune_geo_cache(&mut cache, &seen, now));

    assert!(cache.locations.contains_key("203.0.113.1"));
    assert!(
        !cache.locations.contains_key("203.0.113.2"),
        "an address nobody names and nothing refreshed should go"
    );
    assert!(
        cache.locations.contains_key("203.0.113.3"),
        "an address that merely went quiet should be kept"
    );
}

/// The override file is written by hand, and a typed 552.37 reaches the public
/// map and then throws inside MapLibre when a visitor opens the cluster
/// holding it. Every other source is checked against the globe; this one was
/// checked only for being a number.
#[test]
fn a_manual_override_off_the_globe_is_refused() {
    let dir = std::env::temp_dir().join(format!(
        "validatorclock_manual_globe_{}_{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let chain_dir = dir.join("everscale");
    std::fs::create_dir_all(&chain_dir).unwrap();

    std::fs::write(
        chain_dir.join("203.0.113.7.json"),
        json!({
            "ip": "203.0.113.7",
            "geo": { "latitude": 552.37, "longitude": 4.89, "country": "Nowhere", "city": "Nowhere" }
        })
        .to_string(),
    )
    .unwrap();
    std::fs::write(
        chain_dir.join("203.0.113.8.json"),
        json!({
            "ip": "203.0.113.8",
            "geo": { "latitude": 52.37, "longitude": 4.89, "country": "Netherlands", "city": "Amsterdam" }
        })
        .to_string(),
    )
    .unwrap();

    let resolved = load_manual_resolved_locations(&dir, "everscale");

    assert!(
        !resolved.contains_key(&"203.0.113.7".parse::<IpAddr>().unwrap()),
        "a latitude off the globe should be refused"
    );
    assert!(
        resolved.contains_key(&"203.0.113.8".parse::<IpAddr>().unwrap()),
        "a real override should still be read"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_remembered_address_keeps_the_time_the_dht_confirmed_it() {
    // The resolver offers an address for an hour after the node stopped
    // answering, and says when it last did. Stamping the reading time instead
    // published a node nobody had reached for fifty minutes as seen just now,
    // and every window measured from that moment onwards started again.
    let candidates = collect_candidates_from_value(
        &json!([{
            "validator_public_key": "peer-a",
            "resolution": {
                "status": "remembered",
                "addresses": [{"ip": "203.0.113.10", "port": 3030}],
                "confirmed_at": 1_700_000_000u64
            }
        }]),
        None,
    );

    assert_eq!(
        candidates,
        vec![CandidateNode {
            peer: "peer-a".to_owned(),
            ip: "203.0.113.10".parse().unwrap(),
            confirmed_at: Some(1_700_000_000),
        }]
    );

    let mut cache = GeoCache::default();
    cache.locations.insert(
        "203.0.113.10".to_owned(),
        cached_location("Example City", "Exampleland", "EX"),
    );

    let nodes = build_map_nodes_from_candidates_with_retention(
        &candidates,
        &cache,
        &BTreeMap::new(),
        &PreviousMapNodes::default(),
        1_700_003_000,
    )
    .nodes;

    assert_eq!(nodes.len(), 1);
    assert_eq!(
        nodes[0].last_seen_at, 1_700_000_000,
        "the map should publish when the address was confirmed, not when the file was written"
    );
}

#[test]
fn a_source_that_does_not_date_its_records_falls_back_to_the_reading() {
    // The Tycho collector writes plain addresses with no timestamp. There the
    // reading is the only thing that can be said about freshness, and treating
    // it as the sighting is right rather than a guess.
    let candidates =
        collect_candidates_from_value(&json!([{"peer": "peer-a", "ip": "203.0.113.10"}]), None);
    assert_eq!(candidates[0].confirmed_at, None);

    let mut cache = GeoCache::default();
    cache.locations.insert(
        "203.0.113.10".to_owned(),
        cached_location("Example City", "Exampleland", "EX"),
    );

    let nodes = build_map_nodes_from_candidates_with_retention(
        &candidates,
        &cache,
        &BTreeMap::new(),
        &PreviousMapNodes::default(),
        1_700_003_000,
    )
    .nodes;

    assert_eq!(nodes[0].last_seen_at, 1_700_003_000);
}

#[test]
fn a_remembered_address_expires_an_hour_after_the_dht_confirmed_it() {
    // Not an hour after it was last written into the map file. Those differ by
    // however long the resolver went on offering it, which is what made the
    // two windows run one after the other instead of together.
    let candidates = collect_candidates_from_value(
        &json!([{
            "peer": "peer-a",
            "resolution": {
                "addresses": [{"ip": "203.0.113.10"}],
                "confirmed_at": 1_700_000_000u64
            }
        }]),
        None,
    );
    let mut cache = GeoCache::default();
    cache.locations.insert(
        "203.0.113.10".to_owned(),
        cached_location("Example City", "Exampleland", "EX"),
    );
    let previous_nodes = PreviousMapNodes {
        nodes: build_map_nodes_from_candidates_with_retention(
            &candidates,
            &cache,
            &BTreeMap::new(),
            &PreviousMapNodes::default(),
            1_700_003_000,
        )
        .nodes,
        updated_at: None,
    };

    let built = build_map_nodes_from_candidates_with_retention(
        &[],
        &GeoCache::default(),
        &BTreeMap::new(),
        &previous_nodes,
        1_700_003_601,
    );

    assert_eq!(
        built.retained_node_count, 0,
        "an address confirmed an hour and a second ago should be gone, however \
         recently the file that carried it was written"
    );
}
