use crate::fsutil;
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::warn;

const FILE_MODE: u32 = 0o600;

/// A JSON document kept in memory and written back atomically. Writes happen
/// through [`Snapshot`], which the caller takes while holding the lock and
/// writes after releasing it, so disk I/O never blocks the other readers.
#[derive(Debug)]
pub(super) struct JsonStore<T> {
    path: PathBuf,
    label: &'static str,
    value: T,
    last_saved: u64,
    /// Every snapshot is numbered, and the number of the last one to reach the
    /// disk is kept behind the same lock that orders the writes. Two snapshots
    /// taken in order can otherwise be written in either order, which would
    /// move the file backwards.
    writes: Arc<Mutex<u64>>,
    taken: u64,
}

impl<T> JsonStore<T>
where
    T: Clone + Default + Serialize + DeserializeOwned,
{
    pub(super) fn load(path: PathBuf, label: &'static str) -> Self {
        let value = match read_json(&path) {
            Ok(value) => value,
            Err(error) => {
                warn!(
                    path = %path.display(),
                    error = ?error,
                    "failed to load {label} store; starting with empty {label} state"
                );
                // The first write would replace a file that could not be read.
                // Unreadable is not the same as empty, and only a person can
                // tell what was in it, so it is moved aside first.
                keep_unreadable(&path, label);
                T::default()
            }
        };

        Self {
            path,
            label,
            value,
            last_saved: 0,
            writes: Arc::new(Mutex::new(0)),
            taken: 0,
        }
    }

    pub(super) fn get(&self) -> &T {
        &self.value
    }

    pub(super) fn get_mut(&mut self) -> &mut T {
        &mut self.value
    }

    /// A copy to persist, unless a write within the last `min_interval_seconds`
    /// already covers the change. Pass 0 to always take one.
    pub(super) fn take_snapshot(
        &mut self,
        now: u64,
        min_interval_seconds: u64,
    ) -> Option<Snapshot<T>> {
        if min_interval_seconds > 0 && now.saturating_sub(self.last_saved) < min_interval_seconds {
            return None;
        }
        self.last_saved = now;
        self.taken += 1;

        Some(Snapshot {
            path: self.path.clone(),
            label: self.label,
            value: self.value.clone(),
            sequence: self.taken,
            writes: Arc::clone(&self.writes),
        })
    }
}

pub(super) struct Snapshot<T> {
    path: PathBuf,
    label: &'static str,
    value: T,
    sequence: u64,
    writes: Arc<Mutex<u64>>,
}

impl<T: Serialize + Send + 'static> Snapshot<T> {
    /// Serialising the document and writing it both happen off the runtime: an
    /// atomic write ends in two fsyncs, and a request must not hold a worker
    /// thread while the disk catches up.
    pub(super) async fn write(self) {
        let Snapshot {
            path,
            label,
            value,
            sequence,
            writes,
        } = self;

        let mut written = writes.lock().await;
        if *written >= sequence {
            // A newer snapshot is already on disk; this one would undo it.
            return;
        }

        let write_path = path.clone();
        let persisted = tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
            let content = serde_json::to_vec_pretty(&value)?;
            fsutil::write_file_atomic(&write_path, &content, FILE_MODE)
        })
        .await;

        match persisted {
            Ok(Ok(())) => *written = sequence,
            Ok(Err(error)) => warn!(
                path = %path.display(),
                error = ?error,
                "failed to persist the {label} store"
            ),
            Err(error) => warn!(error = ?error, "the {label} store write task failed"),
        }
    }
}

/// A store that could not be read is moved aside rather than replaced, so the
/// data stays on disk for a person to look at.
fn keep_unreadable(path: &Path, label: &'static str) {
    match fsutil::keep_unreadable_file(path) {
        Ok(kept) => warn!(kept = %kept.display(), "kept the unreadable {label} store aside"),
        Err(error) => warn!(
            path = %path.display(),
            error = ?error,
            "failed to keep the unreadable {label} store aside"
        ),
    }
}

fn read_json<T: Default + DeserializeOwned>(path: &Path) -> anyhow::Result<T> {
    use anyhow::Context;

    match fs::read_to_string(path) {
        Ok(content) => serde_json::from_str(&content)
            .with_context(|| format!("failed to parse {}", path.display())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(T::default()),
        Err(error) => Err(error).with_context(|| format!("failed to read {}", path.display())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
    struct Counters {
        visits: u64,
    }

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "validatorclock_json_store_{name}_{}_{:?}.json",
            std::process::id(),
            std::thread::current().id()
        ))
    }

    #[test]
    fn a_missing_file_loads_as_the_default_value() {
        let path = temp_path("missing");
        let _ = fs::remove_file(&path);

        let store = JsonStore::<Counters>::load(path, "test");

        assert_eq!(store.get(), &Counters::default());
    }

    #[tokio::test]
    async fn snapshots_round_trip_through_the_file() {
        let path = temp_path("round_trip");
        let mut store = JsonStore::<Counters>::load(path.clone(), "test");
        store.get_mut().visits = 7;

        store.take_snapshot(100, 0).unwrap().write().await;
        let reloaded = JsonStore::<Counters>::load(path.clone(), "test");

        assert_eq!(reloaded.get().visits, 7);
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn throttled_snapshots_are_skipped_until_the_interval_passes() {
        let mut store = JsonStore::<Counters>::load(temp_path("throttle"), "test");

        assert!(store.take_snapshot(100, 15).is_some());
        assert!(store.take_snapshot(110, 15).is_none());
        assert!(store.take_snapshot(115, 15).is_some());
        assert!(store.take_snapshot(115, 0).is_some());
    }

    #[test]
    fn an_unreadable_file_falls_back_to_the_default_value() {
        let path = temp_path("broken");
        fs::write(&path, b"{ not json").unwrap();

        let store = JsonStore::<Counters>::load(path.clone(), "test");

        assert_eq!(store.get(), &Counters::default());
        let _ = fs::remove_file(&path);
        remove_kept_files(&path);
    }

    /// Starting empty is right, but the next write would then replace a file
    /// nobody has looked at yet. The unreadable one is kept instead.
    #[test]
    fn an_unreadable_file_is_kept_aside_rather_than_replaced() {
        let path = temp_path("kept");
        fs::write(&path, b"{ not json").unwrap();

        let _store = JsonStore::<Counters>::load(path.clone(), "test");

        assert!(
            !path.exists(),
            "the unreadable file should have been moved out of the way"
        );
        let kept = kept_files(&path);
        assert_eq!(kept.len(), 1, "exactly one copy should be kept: {kept:?}");
        assert_eq!(fs::read(&kept[0]).unwrap(), b"{ not json");

        remove_kept_files(&path);
    }

    /// Snapshots are written after the store lock is released, so two of them
    /// can reach the disk in either order. The older one must not win.
    #[tokio::test]
    async fn a_stale_snapshot_does_not_overwrite_a_newer_one() {
        let path = temp_path("ordering");
        let _ = fs::remove_file(&path);
        let mut store = JsonStore::<Counters>::load(path.clone(), "test");

        store.get_mut().visits = 1;
        let older = store.take_snapshot(100, 0).unwrap();
        store.get_mut().visits = 2;
        let newer = store.take_snapshot(101, 0).unwrap();

        newer.write().await;
        older.write().await;

        let reloaded = JsonStore::<Counters>::load(path.clone(), "test");
        assert_eq!(
            reloaded.get().visits,
            2,
            "the newer snapshot should still be the one on disk"
        );
        let _ = fs::remove_file(&path);
    }

    fn kept_files(path: &Path) -> Vec<PathBuf> {
        let name = path.file_name().and_then(|name| name.to_str()).unwrap();
        let prefix = format!("{name}.unreadable-");
        let dir = path.parent().unwrap();
        let mut kept = fs::read_dir(dir)
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
}
