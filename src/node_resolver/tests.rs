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
            local_addr: "0.0.0.0:4191".to_owned(),
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
                local_addr: None,
                protocol: crate::config::ResolverProtocol::Adnl,
            },
        )]),
        ..NodeResolverConfig::default()
    };

    assert!(config.active_chains().is_empty());
}

/// The second ask is for addresses the DHT was asked about and said nothing
/// to. A validator with no address, or a malformed one, was never asked and
/// asking now would spend a lookup to be told the same thing again.
fn resolved_validator(adnl_addr: Option<&str>, resolution: Resolution) -> ResolvedValidator {
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

fn address(ip: &str) -> dht::ResolvedAddress {
    dht::ResolvedAddress {
        ip: ip.to_owned(),
        port: 40100,
        version: "udp4".to_owned(),
    }
}

fn found_at(ip: &str, confirmed_at: u64) -> Resolution {
    Resolution {
        status: "resolved".to_owned(),
        addresses: vec![address(ip)],
        error: None,
        confirmed_at: Some(confirmed_at),
    }
}

/// A validator as the chain named it, with only the field the resolver reads.
fn chain_validator(adnl_addr: Option<&str>) -> crate::chain::ValidatorDto {
    let mut validator = crate::chain::test_clock_snapshot("ton")
        .current_set
        .validators[0]
        .clone();
    validator.adnl_addr = adnl_addr.map(str::to_owned);
    validator
}

#[test]
fn only_the_lookups_that_came_back_empty_are_asked_again() {
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
        resolved_validator(
            Some(&"b".repeat(64)),
            Resolution::failed_for_test("no answer"),
        ),
        resolved_validator(Some(&"c".repeat(64)), found),
        resolved_validator(None, Resolution::missing_adnl()),
        resolved_validator(Some("xyz"), Resolution::invalid_adnl("xyz")),
        resolved_validator(
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

/// What the chain named settles whether a lookup happens at all, and the file
/// says which of the three it was.
#[test]
fn only_a_validator_the_chain_gave_an_address_for_is_looked_up() {
    let named = "ab".repeat(32);
    assert_eq!(
        address_to_look_up(&chain_validator(Some(&named))).unwrap(),
        named
    );

    let missing = address_to_look_up(&chain_validator(None)).unwrap_err();
    assert_eq!(missing.status, "missing_adnl");
    assert!(!missing.has_address());

    let invalid = address_to_look_up(&chain_validator(Some("xyz"))).unwrap_err();
    assert_eq!(invalid.status, "invalid_adnl");
    assert!(
        invalid
            .error
            .as_deref()
            .is_some_and(|error| error.contains("xyz")),
        "the offending value is named: {invalid:?}"
    );
    assert!(
        !invalid.is_failed(),
        "an address that could never be one is not a lookup that failed, and is not asked again"
    );
}

/// The memory is filled by the passes that reach an address and read by the
/// ones that do not.
#[test]
fn an_address_reached_this_pass_is_kept_and_one_that_was_not_is_offered_back() {
    let mut memory = ResolvedAddressMemory::default();
    memory.remember(&"bb".repeat(32), &address("198.51.100.7"), 1_000);

    let mut resolved = vec![
        resolved_validator(Some(&"aa".repeat(32)), found_at("203.0.113.1", 2_000)),
        resolved_validator(
            Some(&"bb".repeat(32)),
            Resolution::failed_for_test("no answer"),
        ),
        resolved_validator(
            Some(&"cc".repeat(32)),
            Resolution::failed_for_test("no answer"),
        ),
        resolved_validator(None, Resolution::missing_adnl()),
    ];

    apply_remembered_addresses(&mut resolved, &mut memory, 2_000);

    assert_eq!(resolved[0].resolution.status, "resolved");
    assert_eq!(resolved[1].resolution.status, "remembered");
    assert_eq!(resolved[1].resolution.addresses[0].ip, "198.51.100.7");
    assert_eq!(
        resolved[1].resolution.confirmed_at,
        Some(1_000),
        "a remembered address carries when it was last confirmed, not when it was offered again"
    );
    assert_eq!(
        resolved[2].resolution.status, "failed",
        "nothing has ever been known about this one, so there is nothing to offer"
    );
    assert_eq!(resolved[3].resolution.status, "missing_adnl");

    assert_eq!(
        memory
            .recall(&"aa".repeat(32), 2_100)
            .expect("the address this pass reached is what a later one is offered")
            .addresses[0]
            .ip,
        "203.0.113.1"
    );
}

/// The file says how many validators the map can place and how each of them
/// came to be placed - freshly reached, or offered from memory.
#[test]
fn a_pass_counts_what_it_placed_and_how_it_came_to_place_it() {
    let resolved = vec![
        resolved_validator(Some(&"aa".repeat(32)), found_at("203.0.113.1", 2_000)),
        resolved_validator(
            Some(&"bb".repeat(32)),
            Resolution::remembered(address("198.51.100.7"), 1_000),
        ),
        resolved_validator(
            Some(&"cc".repeat(32)),
            Resolution::failed_for_test("no answer"),
        ),
        resolved_validator(Some("xyz"), Resolution::invalid_adnl("xyz")),
        resolved_validator(None, Resolution::missing_adnl()),
    ];

    let totals = PassTotals::of(&resolved);

    assert_eq!(
        totals.resolved, 1,
        "only the one the DHT answered about in this pass"
    );
    assert_eq!(totals.remembered, 1);
    assert_eq!(
        totals.placed, 2,
        "the map shows both, and the file says which is which"
    );
    assert_eq!(
        totals.with_adnl, 4,
        "the chain named an address for four of them, malformed or not"
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
