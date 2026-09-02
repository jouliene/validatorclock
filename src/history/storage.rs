use super::{RoundHistoryDisk, RoundHistoryRetention, RoundHistoryStore};
use crate::fsutil::write_file_atomic;
use anyhow::{Context, Result, bail};
use std::fs;
use std::fs::OpenOptions;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};
use tracing::{info, warn};

#[cfg(test)]
pub(super) fn load_round_history(path: &Path) -> Result<RoundHistoryStore> {
    load_round_history_or_keep_aside(path)
}

pub(crate) fn load_round_history_for_chains<'a>(
    base_path: &Path,
    chain_ids: impl IntoIterator<Item = &'a str>,
) -> RoundHistoryStore {
    let mut history = RoundHistoryStore::default();

    for chain_id in chain_ids {
        let chain_path = round_history_chain_path(base_path, chain_id);
        // One chain that cannot be read must not cost the others their
        // history: this used to return early and start every chain empty.
        let chain_history = match load_round_history_or_keep_aside(&chain_path) {
            Ok(history) => history,
            Err(error) => {
                warn!(
                    chain_id,
                    path = %chain_path.display(),
                    error = ?error,
                    "failed to load chain round history"
                );
                continue;
            }
        };
        if let Some(chain) = chain_history.chains.get(chain_id).cloned() {
            history.chains.insert(chain_id.to_owned(), chain);
        }
    }

    history.remove_incomplete_rounds();
    history
}

/// History that no longer parses is moved aside and the chain starts empty:
/// leaving it in place blocked every later save for that chain, so the chain
/// stopped recording rounds for good.
///
/// A file that could not be *read* is a different matter - it may be perfectly
/// good - so that error is passed on and the caller does not get to replace it.
fn load_round_history_or_keep_aside(path: &Path) -> Result<RoundHistoryStore> {
    match load_round_history_optional(path) {
        Ok(history) => Ok(history.unwrap_or_default()),
        Err(error) if is_unparsable(&error) => {
            warn!(path = %path.display(), error = ?error, "chain round history does not parse");
            match crate::fsutil::keep_unreadable_file(path) {
                Ok(kept) => {
                    warn!(kept = %kept.display(), "kept the unreadable round history aside");
                }
                Err(error) => {
                    warn!(error = ?error, "failed to keep the unreadable round history aside");
                }
            }
            Ok(RoundHistoryStore::default())
        }
        Err(error) => Err(error),
    }
}

fn is_unparsable(error: &anyhow::Error) -> bool {
    error.downcast_ref::<serde_json::Error>().is_some()
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
    let content = serde_json::to_vec(&disk)?;
    write_file_atomic(path, &content, 0o644)
}

pub(crate) fn save_round_history_merged(
    base_path: &Path,
    chain_id: &str,
    history: &RoundHistoryStore,
    retention: &RoundHistoryRetention,
) -> Result<RoundHistoryStore> {
    let path = round_history_chain_path(base_path, chain_id);
    let _lock = RoundHistoryFileLock::acquire(&path)?;
    let mut disk_history = load_round_history_or_keep_aside(&path)?;
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
    crate::fsutil::chain_file_path(base_path, chain_id)
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
                Ok(mut file) => {
                    // Whose lock this is. The holder is killed without running
                    // Drop on every deploy - SIGTERM does not unwind - and
                    // without this the next process could only wait out the
                    // staleness window, answering requests with a timeout the
                    // whole time.
                    let _ = write!(file, "{}", std::process::id());
                    return Ok(Self { path: lock_path });
                }
                Err(error) if error.kind() == ErrorKind::AlreadyExists => {
                    if lock_is_abandoned(&lock_path) {
                        let _ = fs::remove_file(&lock_path);
                        continue;
                    }
                    if started_at.elapsed() > LOCK_WAIT_BUDGET {
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
        .unwrap_or_else(|| ".validatorclock_history.lock".to_owned());
    lock_path.set_file_name(file_name);
    lock_path
}

/// How long a save waits for the lock. The request that asked for it has
/// already given up long before this; waiting further only holds a blocking
/// thread, and the next cycle saves anyway.
const LOCK_WAIT_BUDGET: Duration = Duration::from_secs(15);

/// How long a lock whose holder cannot be identified is honoured.
const LOCK_STALE_AFTER: Duration = Duration::from_secs(300);

/// A lock nobody holds any more: either the process that wrote it is gone, or
/// it is old enough that no live holder would still be working on it.
pub(super) fn lock_is_abandoned(path: &Path) -> bool {
    match lock_holder_pid(path) {
        Some(pid) if pid == std::process::id() => false,
        Some(pid) => !process_is_alive(pid),
        None => lock_file_is_stale(path, LOCK_STALE_AFTER),
    }
}

fn lock_holder_pid(path: &Path) -> Option<u32> {
    fs::read_to_string(path).ok()?.trim().parse().ok()
}

#[cfg(target_os = "linux")]
fn process_is_alive(pid: u32) -> bool {
    Path::new(&format!("/proc/{pid}")).exists()
}

#[cfg(not(target_os = "linux"))]
fn process_is_alive(_pid: u32) -> bool {
    // Without a way to ask, fall back to waiting the lock out.
    true
}

fn lock_file_is_stale(path: &Path, stale_after: Duration) -> bool {
    fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|modified| modified.elapsed().ok())
        .is_some_and(|age| age > stale_after)
}
