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

    let loaded = load_round_history_for_chains(&path, ["everscale", "tycho-testnet"]);

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
            Path::new("/var/lib/validatorclock_history.json"),
            "tycho/testnet"
        ),
        PathBuf::from("/var/lib/validatorclock_history_tycho_testnet.json")
    );
}

#[test]
fn round_history_lock_path_adds_lock_suffix() {
    assert_eq!(
        round_history_lock_path(Path::new("validatorclock_history.json")),
        PathBuf::from("validatorclock_history.json.lock")
    );
}

/// One chain whose file no longer parses used to abort the whole load, so
/// every chain started with an empty history.
#[test]
fn a_chain_whose_file_does_not_parse_does_not_empty_the_others() {
    let path = temp_history_path("corrupt_neighbour");
    let everscale_path = round_history_chain_path(&path, "everscale");
    let tycho_path = round_history_chain_path(&path, "tycho-testnet");

    let mut everscale_history = RoundHistoryStore::default();
    record_rounds(&mut everscale_history, "everscale", &[22, 23]);
    let disk = RoundHistoryDisk {
        version: 1,
        chains: everscale_history.chains,
    };
    fs::write(
        &everscale_path,
        serde_json::to_string_pretty(&disk).unwrap(),
    )
    .unwrap();
    fs::write(&tycho_path, b"{ this is not history").unwrap();

    let loaded = load_round_history_for_chains(&path, ["everscale", "tycho-testnet"]);

    assert_eq!(
        loaded.chains["everscale"]
            .rounds
            .keys()
            .copied()
            .collect::<Vec<_>>(),
        vec![22, 23],
        "a healthy chain should keep its history"
    );
    assert!(!loaded.chains.contains_key("tycho-testnet"));
    assert!(
        !tycho_path.exists(),
        "the unparsable file should have been moved aside"
    );

    remove_kept_files(&tycho_path);
    let _ = fs::remove_file(&everscale_path);
}

/// Leaving the unparsable file in place made every later save for that chain
/// fail on it, so the chain stopped recording rounds for good.
#[test]
fn a_chain_whose_file_does_not_parse_can_save_again() {
    let path = temp_history_path("corrupt_save");
    let chain_path = round_history_chain_path(&path, "everscale");
    fs::write(&chain_path, b"{ this is not history").unwrap();

    let mut history = RoundHistoryStore::default();
    record_rounds(&mut history, "everscale", &[30, 31]);

    let saved = save_round_history_merged(
        &path,
        "everscale",
        &history,
        &RoundHistoryRetention::default(),
    )
    .expect("an unparsable file should not block the save");

    assert_eq!(
        saved.chains["everscale"]
            .rounds
            .keys()
            .copied()
            .collect::<Vec<_>>(),
        vec![30, 31]
    );
    let kept = kept_files(&chain_path);
    assert_eq!(kept.len(), 1, "the old bytes should be kept: {kept:?}");
    assert_eq!(fs::read(&kept[0]).unwrap(), b"{ this is not history");

    remove_kept_files(&chain_path);
    let _ = fs::remove_file(&chain_path);
}

fn kept_files(path: &Path) -> Vec<PathBuf> {
    let name = path.file_name().and_then(|name| name.to_str()).unwrap();
    let prefix = format!("{name}.unreadable-");
    let mut kept = fs::read_dir(path.parent().unwrap())
        .unwrap()
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|entry| {
            entry
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(&prefix))
        })
        .collect::<Vec<_>>();
    kept.sort();
    kept
}

fn remove_kept_files(path: &Path) {
    for kept in kept_files(path) {
        let _ = fs::remove_file(kept);
    }
}
