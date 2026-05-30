use super::super::participation::ValidatorIdentitySet;
use super::*;

#[test]
fn retention_prunes_rounds_outside_visible_windows() {
    let mut store = RoundHistoryStore::default();
    let chain = store.chains.entry("test".to_owned()).or_default();
    for round_id in [10_u32, 12, 14, 16, 18, 20, 21] {
        let color = if round_id.is_multiple_of(2) {
            RoundColor::Blue
        } else {
            RoundColor::Green
        };
        chain.record_set(&set(round_id, color, vec!["alice"]), 100);
    }

    let mut retention = RoundHistoryRetention::default();
    retention.add_round_window("test", 20);
    assert!(store.prune_to_retention(&retention));

    let rounds = store.chains["test"]
        .rounds
        .keys()
        .copied()
        .collect::<Vec<_>>();
    assert_eq!(rounds, vec![12, 14, 16, 18, 20]);
}

#[test]
fn snapshot_retention_keeps_previous_color_window_when_previous_set_is_missing() {
    let mut store = RoundHistoryStore::default();
    record_rounds(
        &mut store,
        "test",
        &[
            27153, 27154, 27155, 27156, 27157, 27158, 27159, 27160, 27161, 27162,
        ],
    );

    let mut snapshot = crate::chain::test_clock_snapshot("test");
    snapshot.current_set = set(27162, RoundColor::Blue, vec!["alice"]);
    snapshot.previous_set = None;
    snapshot.next_set = None;

    let retention = RoundHistoryStore::retention_for_snapshot("test", &snapshot);
    store.prune_to_retention(&retention);

    let rounds = store.chains["test"]
        .rounds
        .keys()
        .copied()
        .collect::<Vec<_>>();
    assert!(rounds.contains(&27153));

    let history = store.same_color_participation("test", 27161, RoundColor::Green, "alice", None);
    assert_eq!(
        history.iter().map(|entry| entry.round).collect::<Vec<_>>(),
        vec![27153, 27155, 27157, 27159, 27161]
    );
    assert!(
        history
            .iter()
            .all(|entry| matches!(entry.status, ParticipationStatus::Participated))
    );
}

#[test]
fn far_even_round_jump_prunes_stale_rounds_and_leaves_unknown_holes() {
    let mut store = RoundHistoryStore::default();
    let chain = store.chains.entry("test".to_owned()).or_default();
    for round_id in [21118_u32, 21120, 21122, 21124] {
        chain.record_set(&set(round_id, RoundColor::Blue, vec!["alice", "bob"]), 100);
    }
    chain.record_set(&set(21200, RoundColor::Blue, vec!["alice"]), 200);

    let mut retention = RoundHistoryRetention::default();
    retention.add_round_window("test", 21200);
    assert!(store.prune_to_retention(&retention));

    let rounds = store.chains["test"]
        .rounds
        .keys()
        .copied()
        .collect::<Vec<_>>();
    assert_eq!(rounds, vec![21200]);

    let alice_history =
        store.same_color_participation("test", 21200, RoundColor::Blue, "alice", None);
    assert_eq!(
        alice_history
            .iter()
            .map(|entry| entry.round)
            .collect::<Vec<_>>(),
        vec![21192, 21194, 21196, 21198, 21200]
    );
    assert!(
        alice_history[..4]
            .iter()
            .all(|entry| matches!(entry.status, ParticipationStatus::Unknown))
    );
    assert!(matches!(
        alice_history[4].status,
        ParticipationStatus::Participated
    ));

    let bob_history = store.same_color_participation("test", 21200, RoundColor::Blue, "bob", None);
    assert!(
        bob_history[..4]
            .iter()
            .all(|entry| matches!(entry.status, ParticipationStatus::Unknown))
    );
    assert!(matches!(bob_history[4].status, ParticipationStatus::Missed));

    let current_validators = ValidatorIdentitySet::from_validators(&[validator("alice")]);
    let absent =
        store.recent_absent_validators("test", 21200, RoundColor::Blue, &current_validators);
    assert!(absent.is_empty());
}

#[test]
fn far_odd_round_jump_uses_same_retention_window_rules() {
    let mut store = RoundHistoryStore::default();
    let chain = store.chains.entry("test".to_owned()).or_default();
    for round_id in [21119_u32, 21121, 21123, 21125] {
        chain.record_set(&set(round_id, RoundColor::Green, vec!["alice", "bob"]), 100);
    }
    chain.record_set(&set(21201, RoundColor::Green, vec!["alice"]), 200);

    let mut retention = RoundHistoryRetention::default();
    retention.add_round_window("test", 21201);
    assert!(store.prune_to_retention(&retention));

    let rounds = store.chains["test"]
        .rounds
        .keys()
        .copied()
        .collect::<Vec<_>>();
    assert_eq!(rounds, vec![21201]);

    let history = store.same_color_participation("test", 21201, RoundColor::Green, "alice", None);
    assert_eq!(
        history.iter().map(|entry| entry.round).collect::<Vec<_>>(),
        vec![21193, 21195, 21197, 21199, 21201]
    );
    assert!(
        history[..4]
            .iter()
            .all(|entry| matches!(entry.status, ParticipationStatus::Unknown))
    );
    assert!(matches!(
        history[4].status,
        ParticipationStatus::Participated
    ));
}

#[test]
fn retained_real_recent_rounds_still_drive_absence_after_gap() {
    let mut store = RoundHistoryStore::default();
    let chain = store.chains.entry("test".to_owned()).or_default();
    chain.record_set(&set(21124, RoundColor::Blue, vec!["carol"]), 100);
    chain.record_set(&set(21198, RoundColor::Blue, vec!["bob"]), 200);
    chain.record_set(&set(21200, RoundColor::Blue, vec!["alice"]), 300);

    let mut retention = RoundHistoryRetention::default();
    retention.add_round_window("test", 21200);
    assert!(store.prune_to_retention(&retention));

    let rounds = store.chains["test"]
        .rounds
        .keys()
        .copied()
        .collect::<Vec<_>>();
    assert_eq!(rounds, vec![21198, 21200]);

    let bob_history = store.same_color_participation("test", 21200, RoundColor::Blue, "bob", None);
    assert!(
        bob_history[..3]
            .iter()
            .all(|entry| matches!(entry.status, ParticipationStatus::Unknown))
    );
    assert!(matches!(
        bob_history[3].status,
        ParticipationStatus::Participated
    ));
    assert!(matches!(bob_history[4].status, ParticipationStatus::Missed));

    let current_validators = ValidatorIdentitySet::from_validators(&[validator("alice")]);
    let absent =
        store.recent_absent_validators("test", 21200, RoundColor::Blue, &current_validators);
    assert_eq!(absent.len(), 1);
    assert_eq!(absent[0].public_key, "bob");
    assert_eq!(absent[0].last_seen_round, 21198);
}
