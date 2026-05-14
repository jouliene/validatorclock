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
fn recording_same_complete_round_is_not_dirty() {
    let mut store = RoundHistoryStore::default();
    let chain = store.chains.entry("test".to_owned()).or_default();

    assert!(chain.record_set(&set(10, RoundColor::Blue, vec!["alice"]), 100));
    assert!(!chain.record_set(&set(10, RoundColor::Blue, vec!["alice"]), 200));
}
