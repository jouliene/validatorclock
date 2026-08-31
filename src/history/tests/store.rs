use super::*;

#[test]
fn merge_ignores_legacy_incomplete_rounds() {
    let mut complete_store = RoundHistoryStore::default();
    complete_store
        .chains
        .entry("test".to_owned())
        .or_default()
        .record_set(&set(10, RoundColor::Blue, vec!["alice", "bob"]), 100);

    let mut legacy_store = RoundHistoryStore::default();
    let legacy_round = set(10, RoundColor::Blue, vec!["alice", "carol"]);
    legacy_store
        .chains
        .entry("test".to_owned())
        .or_default()
        .rounds
        .insert(10, stored_round(&legacy_round, 200, false));

    complete_store.merge_from(legacy_store);
    let round = &complete_store.chains["test"].rounds[&10];

    assert!(round.complete);
    assert!(round.validators.contains_key("alice"));
    assert!(round.validators.contains_key("bob"));
    assert!(!round.validators.contains_key("carol"));
}

#[test]
fn merge_replaces_legacy_incomplete_round_with_complete_round() {
    let mut legacy_store = RoundHistoryStore::default();
    let legacy_round = set(10, RoundColor::Blue, vec!["alice"]);
    legacy_store
        .chains
        .entry("test".to_owned())
        .or_default()
        .rounds
        .insert(10, stored_round(&legacy_round, 100, false));

    let mut complete_store = RoundHistoryStore::default();
    complete_store
        .chains
        .entry("test".to_owned())
        .or_default()
        .record_set(&set(10, RoundColor::Blue, vec!["bob"]), 200);

    legacy_store.merge_from(complete_store);
    let round = &legacy_store.chains["test"].rounds[&10];

    assert!(round.complete);
    assert!(!round.validators.contains_key("alice"));
    assert!(round.validators.contains_key("bob"));
}

#[test]
fn merge_keeps_newer_complete_round_authoritative() {
    let mut newer_store = RoundHistoryStore::default();
    newer_store
        .chains
        .entry("test".to_owned())
        .or_default()
        .record_set(
            &ValidatorSetDto {
                validators: vec![validator_with_wallet("alice", Some("-1:wallet"))],
                ..set(10, RoundColor::Blue, Vec::new())
            },
            200,
        );

    let mut older_store = RoundHistoryStore::default();
    older_store
        .chains
        .entry("test".to_owned())
        .or_default()
        .record_set(&set(10, RoundColor::Blue, vec!["bob"]), 100);

    assert!(!newer_store.merge_from(older_store));
    let round = &newer_store.chains["test"].rounds[&10];

    assert!(round.complete);
    assert!(round.validators.contains_key("alice"));
    assert!(!round.validators.contains_key("bob"));
    assert_eq!(
        round.validators["alice"].wallet.as_deref(),
        Some("-1:wallet")
    );
}

#[test]
fn complete_round_refresh_preserves_existing_wallets() {
    let mut store = RoundHistoryStore::default();
    let chain = store.chains.entry("test".to_owned()).or_default();
    assert!(chain.record_set(
        &ValidatorSetDto {
            validators: vec![validator_with_wallet("alice", Some("-1:wallet"))],
            ..set(10, RoundColor::Blue, Vec::new())
        },
        100,
    ));

    assert!(!chain.record_set(&set(10, RoundColor::Blue, vec!["alice"]), 200));
    let round = &chain.rounds[&10];

    assert_eq!(
        round.validators["alice"].wallet.as_deref(),
        Some("-1:wallet")
    );
    assert_eq!(round.observed_at, 100);
}

#[test]
fn complete_round_refresh_preserves_existing_fake_node_status() {
    let mut store = RoundHistoryStore::default();
    let chain = store.chains.entry("test".to_owned()).or_default();
    assert!(chain.record_set(
        &ValidatorSetDto {
            fake_validator_peers: vec!["alice".to_owned()],
            fake_validator_status_known: true,
            ..set(10, RoundColor::Blue, vec!["alice"])
        },
        100,
    ));

    assert!(!chain.record_set(&set(10, RoundColor::Blue, vec!["alice"]), 200));
    let round = &chain.rounds[&10];

    assert_eq!(round.validators["alice"].fake_node, Some(true));
    assert_eq!(round.observed_at, 100);
}

#[test]
fn complete_round_refresh_preserves_existing_map_node() {
    let mut store = RoundHistoryStore::default();
    let chain = store.chains.entry("test".to_owned()).or_default();
    assert!(chain.record_set(
        &ValidatorSetDto {
            validators: vec![ValidatorDto {
                map_node: Some(map_node(
                    "203.0.113.10",
                    "Test ISP",
                    "Test City",
                    "Testland"
                )),
                ..validator("alice")
            }],
            ..set(10, RoundColor::Blue, Vec::new())
        },
        100,
    ));

    assert!(!chain.record_set(&set(10, RoundColor::Blue, vec!["alice"]), 200));
    let round = &chain.rounds[&10];

    assert_eq!(
        round.validators["alice"].map_node,
        Some(map_node(
            "203.0.113.10",
            "Test ISP",
            "Test City",
            "Testland"
        ))
    );
    assert_eq!(round.observed_at, 100);
}

#[test]
fn complete_round_refresh_preserves_existing_map_node_when_validator_becomes_fake() {
    let mut store = RoundHistoryStore::default();
    let chain = store.chains.entry("test".to_owned()).or_default();
    assert!(chain.record_set(
        &ValidatorSetDto {
            validators: vec![ValidatorDto {
                map_node: Some(map_node(
                    "203.0.113.10",
                    "Test ISP",
                    "Test City",
                    "Testland"
                )),
                ..validator("alice")
            }],
            ..set(10, RoundColor::Blue, Vec::new())
        },
        100,
    ));

    assert!(chain.record_set(
        &ValidatorSetDto {
            fake_validator_peers: vec!["alice".to_owned()],
            fake_validator_status_known: true,
            ..set(10, RoundColor::Blue, vec!["alice"])
        },
        200,
    ));
    let round = &chain.rounds[&10];

    assert_eq!(round.validators["alice"].fake_node, Some(true));
    assert_eq!(
        round.validators["alice"].map_node,
        Some(map_node(
            "203.0.113.10",
            "Test ISP",
            "Test City",
            "Testland"
        ))
    );
    assert_eq!(round.observed_at, 200);
}

#[test]
fn fake_round_records_previous_map_node_as_last_known_location() {
    let mut store = RoundHistoryStore::default();
    let chain = store.chains.entry("test".to_owned()).or_default();
    assert!(chain.record_set(
        &ValidatorSetDto {
            validators: vec![ValidatorDto {
                map_node: Some(map_node(
                    "203.0.113.10",
                    "Test ISP",
                    "Test City",
                    "Testland"
                )),
                ..validator("alice")
            }],
            ..set(8, RoundColor::Blue, Vec::new())
        },
        100,
    ));

    assert!(chain.record_set(
        &ValidatorSetDto {
            fake_validator_peers: vec!["alice".to_owned()],
            fake_validator_status_known: true,
            ..set(10, RoundColor::Blue, vec!["alice"])
        },
        200,
    ));
    let round = &chain.rounds[&10];

    assert_eq!(round.validators["alice"].fake_node, Some(true));
    assert_eq!(
        round.validators["alice"].map_node,
        Some(map_node(
            "203.0.113.10",
            "Test ISP",
            "Test City",
            "Testland"
        ))
    );
}

#[test]
fn complete_round_refresh_replaces_fake_node_status_when_known_again() {
    let mut store = RoundHistoryStore::default();
    let chain = store.chains.entry("test".to_owned()).or_default();
    assert!(chain.record_set(
        &ValidatorSetDto {
            fake_validator_peers: vec!["alice".to_owned()],
            fake_validator_status_known: true,
            ..set(10, RoundColor::Blue, vec!["alice"])
        },
        100,
    ));

    assert!(chain.record_set(
        &ValidatorSetDto {
            fake_validator_peers: Vec::new(),
            fake_validator_status_known: true,
            ..set(10, RoundColor::Blue, vec!["alice"])
        },
        200,
    ));
    let round = &chain.rounds[&10];

    assert_eq!(round.validators["alice"].fake_node, Some(false));
    assert_eq!(round.observed_at, 200);
}

#[test]
fn recording_same_complete_round_is_not_dirty() {
    let mut store = RoundHistoryStore::default();
    let chain = store.chains.entry("test".to_owned()).or_default();

    assert!(chain.record_set(&set(10, RoundColor::Blue, vec!["alice"]), 100));
    assert!(!chain.record_set(&set(10, RoundColor::Blue, vec!["alice"]), 200));
}

#[test]
fn refreshes_within_one_sighting_bucket_leave_the_round_unchanged() {
    let mut chain = ChainRoundHistory::default();
    let mut round = set(10, RoundColor::Blue, vec!["alice", "bob"]);
    for validator in &mut round.validators {
        validator.map_node = Some(map_node("203.0.113.9", "OVH", "Paris", "France"));
    }

    assert!(chain.record_set(&round, 3_000), "the first sighting is new");
    assert!(
        !chain.record_set(&round, 3_000 + 60),
        "a refresh a minute later carries no new data"
    );
    assert!(
        !chain.record_set(&round, 3_000 + 120),
        "nor does the one after it"
    );

    let seen_at = chain.rounds[&10].validators["alice"].map_seen_at;
    assert_eq!(seen_at, Some(3_000), "the sighting is stored per bucket");
}

#[test]
fn a_sighting_in_the_next_bucket_updates_the_round() {
    let mut chain = ChainRoundHistory::default();
    let mut round = set(10, RoundColor::Blue, vec!["alice"]);
    for validator in &mut round.validators {
        validator.map_node = Some(map_node("203.0.113.9", "OVH", "Paris", "France"));
    }
    chain.record_set(&round, 3_000);

    assert!(
        chain.record_set(&round, 3_000 + MAP_SEEN_BUCKET_SECONDS),
        "a sighting in the next bucket is recorded"
    );
    assert_eq!(
        chain.rounds[&10].validators["alice"].map_seen_at,
        Some(3_000 + MAP_SEEN_BUCKET_SECONDS)
    );
}

#[test]
fn real_changes_are_still_recorded_inside_one_bucket() {
    let mut chain = ChainRoundHistory::default();
    let round = set(10, RoundColor::Blue, vec!["alice"]);
    chain.record_set(&round, 3_000);

    let grown = set(10, RoundColor::Blue, vec!["alice", "bob"]);

    assert!(
        chain.record_set(&grown, 3_000 + 30),
        "a new validator is new data whatever the bucket says"
    );
    assert!(chain.rounds[&10].validators.contains_key("bob"));
}
