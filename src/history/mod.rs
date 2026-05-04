use crate::chain::RoundColor;

const HISTORY_DEPTH: usize = 5;

mod participation;
mod retention;
mod storage;
mod store;
mod types;

pub(crate) use retention::RoundHistoryRetention;
pub(crate) use storage::{
    load_round_history_for_chains, round_history_chain_path, save_round_history_merged,
};
use types::{ChainRoundHistory, RoundHistoryDisk, StoredRound, StoredValidator};
pub(crate) use types::{
    ParticipationStatus, RecentAbsentValidatorDto, RoundHistoryStore, ValidatorParticipationDto,
};

fn same_color_rounds(round_id: u32) -> Vec<u32> {
    (0..HISTORY_DEPTH)
        .rev()
        .filter_map(|index| round_id.checked_sub((index * 2) as u32))
        .collect()
}

fn opposite_round_color(round_color: RoundColor) -> RoundColor {
    match round_color {
        RoundColor::Blue => RoundColor::Green,
        RoundColor::Green => RoundColor::Blue,
    }
}

#[cfg(test)]
mod tests {
    use super::participation::ValidatorIdentitySet;
    use super::*;
    use crate::chain::{RoundColor, ValidatorDto, ValidatorSetDto};
    use crate::history::storage::{load_round_history, round_history_lock_path};
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn validator(public_key: &str) -> ValidatorDto {
        validator_with_wallet(public_key, None)
    }

    fn validator_with_wallet(public_key: &str, wallet: Option<&str>) -> ValidatorDto {
        ValidatorDto {
            public_key: public_key.to_owned(),
            adnl_addr: None,
            wallet: wallet.map(str::to_owned),
            source: None,
            contract_type: None,
            contract_type_hash: None,
            stake: None,
            reward: None,
            weight: "1".to_owned(),
            weight_percent: 100.0,
            history: Vec::new(),
        }
    }

    fn set(round_id: u32, round_color: RoundColor, validators: Vec<&str>) -> ValidatorSetDto {
        ValidatorSetDto {
            utime_since: round_id * 10,
            utime_until: round_id * 10 + 10,
            round_id,
            round_color,
            total: validators.len(),
            main: validators.len() as u16,
            total_weight: validators.len().to_string(),
            total_stake: None,
            total_reward: None,
            validators: validators.into_iter().map(validator).collect(),
            recent_absent_validators: Vec::new(),
        }
    }

    fn stored_round(set: &ValidatorSetDto, observed_at: u64, complete: bool) -> StoredRound {
        StoredRound {
            round_id: set.round_id,
            round_color: set.round_color,
            utime_since: set.utime_since,
            utime_until: set.utime_until,
            observed_at,
            complete,
            validators: set
                .validators
                .iter()
                .map(|validator| {
                    (
                        validator.public_key.clone(),
                        StoredValidator {
                            wallet: validator.wallet.clone(),
                        },
                    )
                })
                .collect(),
        }
    }

    fn record_rounds(store: &mut RoundHistoryStore, chain_id: &str, rounds: &[u32]) {
        let chain = store.chains.entry(chain_id.to_owned()).or_default();
        for round_id in rounds {
            let color = if round_id.is_multiple_of(2) {
                RoundColor::Blue
            } else {
                RoundColor::Green
            };
            chain.record_set(&set(*round_id, color, vec!["alice"]), 100);
        }
    }

    fn temp_history_path(test_name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "validators_clock_{test_name}_{}_{}.json",
            std::process::id(),
            unique
        ))
    }

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
        let absent =
            store.recent_absent_validators("test", 10, RoundColor::Blue, &current_validators);
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
        let absent =
            store.recent_absent_validators("test", 10, RoundColor::Blue, &current_validators);

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
        let absent =
            store.recent_absent_validators("test", 10, RoundColor::Blue, &current_validators);

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
        let absent =
            store.recent_absent_validators("test", 10, RoundColor::Blue, &current_validators);

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

        let history = store.same_color_participation(
            "test",
            10,
            RoundColor::Blue,
            "new-key",
            Some("-1:wallet"),
        );

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
        let absent =
            store.recent_absent_validators("test", 10, RoundColor::Blue, &current_validators);

        assert!(absent.is_empty());
    }

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

        let bob_history =
            store.same_color_participation("test", 21200, RoundColor::Blue, "bob", None);
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

        let history =
            store.same_color_participation("test", 21201, RoundColor::Green, "alice", None);
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

        let bob_history =
            store.same_color_participation("test", 21200, RoundColor::Blue, "bob", None);
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

    #[test]
    fn saving_one_chain_does_not_reintroduce_stale_rounds_for_another_chain() {
        let path = temp_history_path("chain_scoped_save");
        let everscale_path = round_history_chain_path(&path, "everscale");
        let tycho_path = round_history_chain_path(&path, "tycho-testnet");

        let mut everscale_history = RoundHistoryStore::default();
        record_rounds(
            &mut everscale_history,
            "everscale",
            &[18, 19, 20, 21, 22, 23, 24, 25, 26, 27],
        );
        let disk = RoundHistoryDisk {
            version: 1,
            chains: everscale_history.chains.clone(),
        };
        fs::write(
            &everscale_path,
            serde_json::to_string_pretty(&disk).unwrap(),
        )
        .unwrap();

        let mut tycho_history = RoundHistoryStore::default();
        record_rounds(&mut tycho_history, "tycho-testnet", &[20, 22, 24, 26]);
        let disk = RoundHistoryDisk {
            version: 1,
            chains: tycho_history.chains,
        };
        fs::write(&tycho_path, serde_json::to_string_pretty(&disk).unwrap()).unwrap();

        let mut stale_clone = everscale_history;
        record_rounds(
            &mut stale_clone,
            "everscale",
            &[6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17],
        );
        record_rounds(&mut stale_clone, "tycho-testnet", &[28]);

        let mut retention = RoundHistoryRetention::default();
        retention.add_round_window("tycho-testnet", 28);
        save_round_history_merged(&path, "tycho-testnet", &stale_clone, &retention).unwrap();

        let saved_everscale = load_round_history(&everscale_path).unwrap();
        let everscale_rounds = saved_everscale.chains["everscale"]
            .rounds
            .keys()
            .copied()
            .collect::<Vec<_>>();
        assert_eq!(
            everscale_rounds,
            vec![18, 19, 20, 21, 22, 23, 24, 25, 26, 27]
        );

        let saved_tycho = load_round_history(&tycho_path).unwrap();
        assert!(!saved_tycho.chains.contains_key("everscale"));
        let tycho_rounds = saved_tycho.chains["tycho-testnet"]
            .rounds
            .keys()
            .copied()
            .collect::<Vec<_>>();
        assert_eq!(tycho_rounds, vec![20, 22, 24, 26, 28]);

        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(&everscale_path);
        let _ = fs::remove_file(&tycho_path);
        let _ = fs::remove_file(round_history_lock_path(&path));
        let _ = fs::remove_file(round_history_lock_path(&everscale_path));
        let _ = fs::remove_file(round_history_lock_path(&tycho_path));
    }

    #[test]
    fn load_round_history_for_chains_ignores_legacy_combined_file() {
        let path = temp_history_path("split_history_load");
        let everscale_path = round_history_chain_path(&path, "everscale");

        let mut legacy_history = RoundHistoryStore::default();
        record_rounds(&mut legacy_history, "everscale", &[18]);
        record_rounds(&mut legacy_history, "tycho-testnet", &[20]);
        let legacy_disk = RoundHistoryDisk {
            version: 1,
            chains: legacy_history.chains,
        };
        fs::write(&path, serde_json::to_string_pretty(&legacy_disk).unwrap()).unwrap();

        let mut everscale_history = RoundHistoryStore::default();
        record_rounds(&mut everscale_history, "everscale", &[22]);
        let everscale_disk = RoundHistoryDisk {
            version: 1,
            chains: everscale_history.chains,
        };
        fs::write(
            &everscale_path,
            serde_json::to_string_pretty(&everscale_disk).unwrap(),
        )
        .unwrap();

        let loaded = load_round_history_for_chains(&path, ["everscale", "tycho-testnet"]).unwrap();

        let everscale_rounds = loaded.chains["everscale"]
            .rounds
            .keys()
            .copied()
            .collect::<Vec<_>>();
        assert_eq!(everscale_rounds, vec![22]);

        assert!(!loaded.chains.contains_key("tycho-testnet"));

        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(&everscale_path);
        let _ = fs::remove_file(round_history_lock_path(&path));
        let _ = fs::remove_file(round_history_lock_path(&everscale_path));
    }

    #[test]
    fn round_history_chain_path_adds_chain_id_before_extension() {
        assert_eq!(
            round_history_chain_path(
                Path::new("/var/lib/validators_clock_history.json"),
                "tycho/testnet"
            ),
            PathBuf::from("/var/lib/validators_clock_history_tycho_testnet.json")
        );
    }

    #[test]
    fn round_history_lock_path_adds_lock_suffix() {
        assert_eq!(
            round_history_lock_path(Path::new("validators_clock_history.json")),
            PathBuf::from("validators_clock_history.json.lock")
        );
    }
}
