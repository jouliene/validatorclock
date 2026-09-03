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
            },
        )]),
        ..NodeResolverConfig::default()
    };

    assert!(config.active_chains().is_empty());
}
