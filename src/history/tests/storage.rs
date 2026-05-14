use super::*;
use crate::history::storage::{load_round_history, round_history_lock_path};
use std::fs;
use std::path::{Path, PathBuf};

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
