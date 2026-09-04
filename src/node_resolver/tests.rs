use super::*;
use crate::config::NodeResolverChainConfig;
use std::collections::HashMap;
use std::path::PathBuf;

/// The output has to stay readable by the node location map, which finds
/// addresses by walking the JSON for a `validators` array of records that
/// carry a peer id and an IP. That contract is what this checks: the file
/// this module writes, parsed by the code that consumes it.
#[test]
fn the_written_set_is_one_the_location_map_can_read() {
    let output = ResolvedSet {
        schema_version: SCHEMA_VERSION,
        chain_id: "everscale".to_owned(),
        fetched_at: 1_788_400_000,
        generated_at: 1_788_400_010,
        round_id: 27_288,
        validators_total: 2,
        validators_main: 2,
        validators_with_adnl: 2,
        resolved_total: 1,
        remembered_total: 0,
        placed_total: 1,
        resolver: ResolverMetadata {
            local_adnl_addr: "0.0.0.0:4191".to_owned(),
            bootstrap_nodes: 3,
        },
        validators: vec![
            ResolvedValidator {
                validator_public_key: "a".repeat(64),
                adnl_addr: Some("b".repeat(64)),
                wallet: None,
                source_address: None,
                source_contract_type_hash: None,
                contract_type: None,
                stake: None,
                weight: Some("1".to_owned()),
                resolution: Resolution {
                    status: "resolved".to_owned(),
                    addresses: vec![dht::ResolvedAddress {
                        ip: "104.238.222.200".to_owned(),
                        port: 40100,
                        version: "udp4".to_owned(),
                    }],
                    error: None,
                    confirmed_at: Some(1_788_400_005),
                },
            },
            ResolvedValidator {
                validator_public_key: "c".repeat(64),
                adnl_addr: Some("d".repeat(64)),
                wallet: None,
                source_address: None,
                source_contract_type_hash: None,
                contract_type: None,
                stake: None,
                weight: Some("1".to_owned()),
                resolution: Resolution::failed_for_test("address not found"),
            },
        ],
    };

    let body = serde_json::to_string(&output).unwrap();
    let value: serde_json::Value = serde_json::from_str(&body).unwrap();
    let candidates = crate::node_locations::candidates_from_value_for_test(&value);

    assert_eq!(
        candidates.len(),
        1,
        "only the validator that was found has an address to place: {candidates:?}"
    );
    assert_eq!(candidates[0].0, "a".repeat(64));
    assert_eq!(candidates[0].1, "104.238.222.200");
    assert_eq!(
        candidates[0].2,
        Some(1_788_400_005),
        "the map has to be able to tell when the address was confirmed, or it          dates every node by when it wrote the file"
    );
}

#[test]
fn a_validator_the_dht_could_not_place_is_recorded_with_its_reason() {
    let body = serde_json::to_string(&Resolution::missing_adnl()).unwrap();
    assert!(body.contains("missing_adnl"));
    assert!(body.contains("no adnl_addr"));

    let body = serde_json::to_string(&Resolution::invalid_adnl("xyz")).unwrap();
    assert!(body.contains("invalid_adnl"));
    assert!(body.contains("xyz"), "the offending value is named: {body}");
}

#[test]
fn nothing_runs_for_a_chain_that_was_not_asked_for() {
    let config = NodeResolverConfig {
        enabled: true,
        chains: HashMap::from([(
            "everscale".to_owned(),
            NodeResolverChainConfig {
                enabled: false,
                global_config_path: Some(PathBuf::from("/tmp/global.json")),
                output_path: Some(PathBuf::from("/tmp/out.json")),
                local_adnl_addr: None,
            },
        )]),
        ..NodeResolverConfig::default()
    };

    assert!(config.active_chains().is_empty());
}

/// The second ask is for addresses the DHT was asked about and said nothing
/// to. A validator with no address, or a malformed one, was never asked and
/// asking now would spend a lookup to be told the same thing again.
#[test]
fn only_the_lookups_that_came_back_empty_are_asked_again() {
    fn validator(adnl_addr: Option<&str>, resolution: Resolution) -> ResolvedValidator {
        ResolvedValidator {
            validator_public_key: "a".repeat(64),
            adnl_addr: adnl_addr.map(str::to_owned),
            wallet: None,
            source_address: None,
            source_contract_type_hash: None,
            contract_type: None,
            stake: None,
            weight: None,
            resolution,
        }
    }

    let found = Resolution {
        status: "resolved".to_owned(),
        addresses: vec![dht::ResolvedAddress {
            ip: "104.238.222.200".to_owned(),
            port: 40100,
            version: "udp4".to_owned(),
        }],
        error: None,
        confirmed_at: Some(1_788_400_000),
    };

    let resolved = vec![
        validator(
            Some(&"b".repeat(64)),
            Resolution::failed_for_test("no answer"),
        ),
        validator(Some(&"c".repeat(64)), found),
        validator(None, Resolution::missing_adnl()),
        validator(Some("xyz"), Resolution::invalid_adnl("xyz")),
        validator(
            Some(&"d".repeat(64)),
            Resolution::failed_for_test("no answer"),
        ),
    ];

    let misses = misses_worth_asking_again(&resolved);
    assert_eq!(
        misses,
        vec![(0, "b".repeat(64)), (4, "d".repeat(64))],
        "the two empty lookups, by position and address: {misses:?}"
    );
}

/// A miss and a validator that had nothing to look up read the same on the
/// map - no address - and must not read the same to the resolver, which asks
/// the first kind again and not the second.
#[test]
fn a_lookup_that_found_nothing_is_told_apart_from_one_never_made() {
    assert!(Resolution::failed_for_test("no answer").is_failed());
    assert!(!Resolution::missing_adnl().is_failed());
    assert!(!Resolution::invalid_adnl("xyz").is_failed());
}

/// The sweep's socket is taken for as long as the resolver lives, so the
/// client built for a second ask binds elsewhere - on the same interface, on a
/// port that is free and, unlike port zero, has a number to announce.
#[test]
fn the_second_ask_binds_a_named_port_on_the_same_interface() {
    assert_eq!(host_of("0.0.0.0:4291"), "0.0.0.0");
    assert_eq!(host_of("127.0.0.1:4191"), "127.0.0.1");
    assert_eq!(
        host_of("0.0.0.0"),
        "0.0.0.0",
        "an address written without a port is all interface"
    );

    let port = free_port("127.0.0.1").expect("the system has a port to spare");
    assert_ne!(port, 0, "port zero is what this exists to avoid");
    assert!(
        std::net::UdpSocket::bind(("127.0.0.1", port)).is_ok(),
        "the port it offers is one that can actually be opened"
    );
}
