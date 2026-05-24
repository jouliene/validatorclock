use super::super::participation::ValidatorIdentitySet;
use super::*;

#[test]
fn same_color_participation_marks_known_misses_and_unknowns() {
    let mut store = RoundHistoryStore::default();
    let chain = store.chains.entry("test".to_owned()).or_default();
    chain.record_set(&set(10, RoundColor::Blue, vec!["alice"]), 100);
    chain.record_set(&set(8, RoundColor::Blue, vec!["alice"]), 100);
    chain.record_set(&set(6, RoundColor::Blue, vec!["bob"]), 100);

    let history = store.same_color_participation("test", 10, RoundColor::Blue, "alice", None);
    assert!(matches!(history[0].status, ParticipationStatus::Unknown));
    assert!(matches!(history[1].status, ParticipationStatus::Unknown));
    assert!(matches!(history[2].status, ParticipationStatus::Missed));
    assert!(matches!(
        history[3].status,
        ParticipationStatus::Participated
    ));
    assert!(matches!(
        history[4].status,
        ParticipationStatus::Participated
    ));
}

#[test]
fn recent_absent_validators_lists_recent_participants_missing_now() {
    let mut store = RoundHistoryStore::default();
    let chain = store.chains.entry("test".to_owned()).or_default();
    chain.record_set(&set(10, RoundColor::Blue, vec!["alice"]), 100);
    chain.record_set(&set(8, RoundColor::Blue, vec!["alice", "bob"]), 100);
    chain.record_set(&set(6, RoundColor::Blue, vec!["carol"]), 100);

    let current_validators = ValidatorIdentitySet::from_validators(&[validator("alice")]);
    let absent = store.recent_absent_validators("test", 10, RoundColor::Blue, &current_validators);
    assert_eq!(absent.len(), 2);
    assert_eq!(absent[0].public_key, "bob");
    assert_eq!(absent[0].last_seen_round, 8);
    assert_eq!(absent[1].public_key, "carol");
    assert_eq!(absent[1].last_seen_round, 6);
    assert!(matches!(
        absent[0].history[4].status,
        ParticipationStatus::Missed
    ));
}

#[test]
fn recent_absent_validators_show_prior_participation_and_current_miss() {
    let mut store = RoundHistoryStore::default();
    let chain = store.chains.entry("test".to_owned()).or_default();
    chain.record_set(&set(2, RoundColor::Blue, vec!["alice"]), 100);
    chain.record_set(&set(4, RoundColor::Blue, vec!["bob"]), 100);
    chain.record_set(&set(6, RoundColor::Blue, vec!["alice"]), 100);
    chain.record_set(&set(8, RoundColor::Blue, vec!["bob"]), 100);
    chain.record_set(&set(10, RoundColor::Blue, vec!["alice"]), 100);

    let current_validators = ValidatorIdentitySet::from_validators(&[validator("alice")]);
    let absent = store.recent_absent_validators("test", 10, RoundColor::Blue, &current_validators);

    assert_eq!(absent.len(), 1);
    assert_eq!(absent[0].public_key, "bob");
    assert_eq!(absent[0].last_seen_round, 8);
    assert_eq!(
        absent[0]
            .history
            .iter()
            .map(|entry| entry.round)
            .collect::<Vec<_>>(),
        vec![2, 4, 6, 8, 10]
    );
    assert!(matches!(
        absent[0].history[0].status,
        ParticipationStatus::Missed
    ));
    assert!(matches!(
        absent[0].history[1].status,
        ParticipationStatus::Participated
    ));
    assert!(matches!(
        absent[0].history[2].status,
        ParticipationStatus::Missed
    ));
    assert!(matches!(
        absent[0].history[3].status,
        ParticipationStatus::Participated
    ));
    assert!(matches!(
        absent[0].history[4].status,
        ParticipationStatus::Missed
    ));
}

#[test]
fn recent_absent_validators_exclude_full_window_misses() {
    let mut store = RoundHistoryStore::default();
    let chain = store.chains.entry("test".to_owned()).or_default();
    chain.record_set(&set(0, RoundColor::Blue, vec!["bob"]), 100);
    for round_id in [2_u32, 4, 6, 8, 10] {
        chain.record_set(&set(round_id, RoundColor::Blue, vec!["alice"]), 100);
    }

    let current_validators = ValidatorIdentitySet::from_validators(&[validator("alice")]);
    let absent = store.recent_absent_validators("test", 10, RoundColor::Blue, &current_validators);

    assert!(absent.is_empty());
}

#[test]
fn legacy_incomplete_round_history_never_marks_missing_validators_as_missed() {
    let mut store = RoundHistoryStore::default();
    let chain = store.chains.entry("test".to_owned()).or_default();
    chain.record_set(&set(10, RoundColor::Blue, vec!["alice"]), 100);
    let legacy_round = set(8, RoundColor::Blue, vec!["bob"]);
    chain
        .rounds
        .insert(8, stored_round(&legacy_round, 100, false));

    let history = store.same_color_participation("test", 10, RoundColor::Blue, "alice", None);

    assert!(matches!(history[3].status, ParticipationStatus::Unknown));
    assert!(matches!(
        history[4].status,
        ParticipationStatus::Participated
    ));
}

#[test]
fn legacy_incomplete_round_history_does_not_create_recent_absent_validators() {
    let mut store = RoundHistoryStore::default();
    let chain = store.chains.entry("test".to_owned()).or_default();
    chain.record_set(&set(10, RoundColor::Blue, vec!["alice"]), 100);
    let legacy_round = set(8, RoundColor::Blue, vec!["bob"]);
    chain
        .rounds
        .insert(8, stored_round(&legacy_round, 100, false));

    let current_validators = ValidatorIdentitySet::from_validators(&[validator("alice")]);
    let absent = store.recent_absent_validators("test", 10, RoundColor::Blue, &current_validators);

    assert!(absent.is_empty());
}

#[test]
fn participation_matches_rotated_public_keys_by_wallet() {
    let mut store = RoundHistoryStore::default();
    let chain = store.chains.entry("test".to_owned()).or_default();
    chain.record_set(
        &ValidatorSetDto {
            validators: vec![validator_with_wallet("old-key", Some("-1:wallet"))],
            ..set(8, RoundColor::Blue, Vec::new())
        },
        100,
    );
    chain.record_set(
        &ValidatorSetDto {
            validators: vec![validator_with_wallet("new-key", Some("-1:wallet"))],
            ..set(10, RoundColor::Blue, Vec::new())
        },
        200,
    );

    let history =
        store.same_color_participation("test", 10, RoundColor::Blue, "new-key", Some("-1:wallet"));

    assert!(matches!(
        history[3].status,
        ParticipationStatus::Participated
    ));
    assert!(matches!(
        history[4].status,
        ParticipationStatus::Participated
    ));
}

#[test]
fn participation_marks_fake_node_rounds() {
    let mut store = RoundHistoryStore::default();
    let chain = store.chains.entry("test".to_owned()).or_default();
    chain.record_set(
        &ValidatorSetDto {
            fake_validator_peers: vec!["alice".to_owned()],
            fake_validator_status_known: true,
            ..set(10, RoundColor::Blue, vec!["alice", "bob"])
        },
        100,
    );

    let alice_history = store.same_color_participation("test", 10, RoundColor::Blue, "alice", None);
    let bob_history = store.same_color_participation("test", 10, RoundColor::Blue, "bob", None);

    assert!(matches!(
        alice_history[4].status,
        ParticipationStatus::Participated
    ));
    assert!(alice_history[4].fake_node);
    assert!(matches!(
        bob_history[4].status,
        ParticipationStatus::Participated
    ));
    assert!(!bob_history[4].fake_node);
}

#[test]
fn recent_absent_validators_uses_wallet_identity() {
    let mut store = RoundHistoryStore::default();
    let chain = store.chains.entry("test".to_owned()).or_default();
    chain.record_set(
        &ValidatorSetDto {
            validators: vec![validator_with_wallet("old-key", Some("-1:wallet"))],
            ..set(8, RoundColor::Blue, Vec::new())
        },
        100,
    );

    let current_validators = ValidatorIdentitySet::from_validators(&[validator_with_wallet(
        "new-key",
        Some("-1:wallet"),
    )]);
    let absent = store.recent_absent_validators("test", 10, RoundColor::Blue, &current_validators);

    assert!(absent.is_empty());
}

#[test]
fn fake_validator_status_is_replayed_to_annotated_sets() {
    let mut store = RoundHistoryStore::default();
    store
        .chains
        .entry("test".to_owned())
        .or_default()
        .record_set(
            &ValidatorSetDto {
                fake_validator_peers: vec!["bob".to_owned()],
                fake_validator_status_known: true,
                ..set(10, RoundColor::Blue, vec!["alice", "bob"])
            },
            100,
        );

    let mut snapshot = crate::chain::test_clock_snapshot("test");
    snapshot.current_set = set(12, RoundColor::Green, vec!["carol"]);
    snapshot.previous_set = Some(set(10, RoundColor::Blue, vec!["alice", "bob"]));

    store.annotate_snapshot("test", &mut snapshot);

    assert_eq!(
        snapshot.previous_set.unwrap().fake_validator_peers,
        vec!["bob".to_owned()]
    );
}

#[test]
fn map_node_is_replayed_to_annotated_sets() {
    let mut store = RoundHistoryStore::default();
    store
        .chains
        .entry("test".to_owned())
        .or_default()
        .record_set(
            &ValidatorSetDto {
                validators: vec![ValidatorDto {
                    map_node: Some(map_node(
                        "203.0.113.10",
                        "Test ISP",
                        "Test City",
                        "Testland",
                    )),
                    ..validator("alice")
                }],
                ..set(10, RoundColor::Blue, Vec::new())
            },
            100,
        );

    let mut snapshot = crate::chain::test_clock_snapshot("test");
    snapshot.current_set = set(12, RoundColor::Green, vec!["bob"]);
    snapshot.previous_set = Some(set(10, RoundColor::Blue, vec!["alice"]));

    store.annotate_snapshot("test", &mut snapshot);

    assert_eq!(
        snapshot.previous_set.unwrap().validators[0].map_node,
        Some(map_node(
            "203.0.113.10",
            "Test ISP",
            "Test City",
            "Testland"
        ))
    );
}
