use super::{RoundHistoryDisk, RoundHistoryRetention, RoundHistoryStore};
use crate::fsutil::write_file_atomic;
use anyhow::{Context, Result, bail};
use std::fs;
use std::fs::OpenOptions;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};
use tracing::info;

#[cfg(test)]
pub(super) fn load_round_history(path: &Path) -> Result<RoundHistoryStore> {
    Ok(load_round_history_optional(path)?.unwrap_or_default())
}

pub(crate) fn load_round_history_for_chains<'a>(
    base_path: &Path,
    chain_ids: impl IntoIterator<Item = &'a str>,
) -> Result<RoundHistoryStore> {
    let mut history = RoundHistoryStore::default();

    for chain_id in chain_ids {
        let chain_path = round_history_chain_path(base_path, chain_id);
        let chain_history = load_round_history_optional(&chain_path)?.unwrap_or_default();
        if let Some(chain) = chain_history.chains.get(chain_id).cloned() {
            history.chains.insert(chain_id.to_owned(), chain);
        }
    }

    history.remove_incomplete_rounds();
    Ok(history)
}

fn load_round_history_optional(path: &Path) -> Result<Option<RoundHistoryStore>> {
    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            return Ok(None);
        }
        Err(error) => {
            return Err(error).with_context(|| format!("failed to read {}", path.display()));
        }
    };
    let disk: RoundHistoryDisk = serde_json::from_str(&content)
        .with_context(|| format!("failed to parse {}", path.display()))?;
    let mut history = RoundHistoryStore {
        chains: disk.chains,
    };
    history.remove_incomplete_rounds();
    Ok(Some(history))
}

fn save_round_history(path: &Path, history: &RoundHistoryStore) -> Result<()> {
    let disk = RoundHistoryDisk {
        version: 1,
        chains: history.chains.clone(),
    };
    let content = serde_json::to_string_pretty(&disk)?;
    write_file_atomic(path, content.as_bytes(), 0o644)
}

pub(crate) fn save_round_history_merged(
    base_path: &Path,
    chain_id: &str,
    history: &RoundHistoryStore,
    retention: &RoundHistoryRetention,
) -> Result<RoundHistoryStore> {
    let path = round_history_chain_path(base_path, chain_id);
    let _lock = RoundHistoryFileLock::acquire(&path)?;
    let mut disk_history = load_round_history_optional(&path)?.unwrap_or_default();
    let rounds_before = disk_history.round_count_for_chain(chain_id);
    disk_history
        .chains
        .retain(|disk_chain_id, _| disk_chain_id == chain_id);

    if let Some(chain) = history.chains.get(chain_id).cloned() {
        disk_history
            .chains
            .entry(chain_id.to_owned())
            .or_default()
            .merge_from(chain);
    }

    let pruned = disk_history.prune_to_retention(retention);
    save_round_history(&path, &disk_history)?;
    info!(
        chain_id,
        path = %path.display(),
        rounds_before,
        rounds_after = disk_history.round_count_for_chain(chain_id),
        pruned,
        "saved chain round history"
    );

    Ok(disk_history)
}

pub(crate) fn round_history_chain_path(base_path: &Path, chain_id: &str) -> PathBuf {
    let safe_chain_id = sanitize_chain_id(chain_id);
    let mut path = base_path.to_path_buf();

    let file_name = base_path
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| {
            let stem = base_path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or(name);
            let extension = base_path
                .extension()
                .and_then(|extension| extension.to_str());
            match extension {
                Some(extension) if !extension.is_empty() => {
                    format!("{stem}_{safe_chain_id}.{extension}")
                }
                _ => format!("{name}_{safe_chain_id}"),
            }
        })
        .unwrap_or_else(|| format!("validators_clock_history_{safe_chain_id}.json"));

    path.set_file_name(file_name);
    path
}

fn sanitize_chain_id(chain_id: &str) -> String {
    let sanitized = chain_id
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();

    if sanitized.is_empty() {
        "chain".to_owned()
    } else {
        sanitized
    }
}

struct RoundHistoryFileLock {
    path: PathBuf,
}

impl RoundHistoryFileLock {
    fn acquire(history_path: &Path) -> Result<Self> {
        if let Some(parent) = history_path.parent()
            && !parent.as_os_str().is_empty()
        {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }

        let lock_path = round_history_lock_path(history_path);
        let started_at = Instant::now();
        loop {
            match OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&lock_path)
            {
                Ok(file) => {
                    let _ = file.set_len(0);
                    return Ok(Self { path: lock_path });
                }
                Err(error) if error.kind() == ErrorKind::AlreadyExists => {
                    if lock_file_is_stale(&lock_path, Duration::from_secs(300)) {
                        let _ = fs::remove_file(&lock_path);
                        continue;
                    }
                    if started_at.elapsed() > Duration::from_secs(120) {
                        bail!("timed out waiting for {}", lock_path.display());
                    }
                    thread::sleep(Duration::from_millis(100));
                }
                Err(error) => {
                    return Err(error)
                        .with_context(|| format!("failed to lock {}", history_path.display()));
                }
            }
        }
    }
}

impl Drop for RoundHistoryFileLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

pub(super) fn round_history_lock_path(history_path: &Path) -> PathBuf {
    let mut lock_path = history_path.to_path_buf();
    let file_name = history_path
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| format!("{name}.lock"))
        .unwrap_or_else(|| ".validators_clock_history.lock".to_owned());
    lock_path.set_file_name(file_name);
    lock_path
}

fn lock_file_is_stale(path: &Path, stale_after: Duration) -> bool {
    fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|modified| modified.elapsed().ok())
        .is_some_and(|age| age > stale_after)
}
