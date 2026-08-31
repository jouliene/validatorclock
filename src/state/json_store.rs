use crate::fsutil;
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::fs;
use std::path::{Path, PathBuf};
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
                T::default()
            }
        };

        Self {
            path,
            label,
            value,
            last_saved: 0,
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

        Some(Snapshot {
            path: self.path.clone(),
            label: self.label,
            value: self.value.clone(),
        })
    }
}

pub(super) struct Snapshot<T> {
    path: PathBuf,
    label: &'static str,
    value: T,
}

impl<T: Serialize> Snapshot<T> {
    pub(super) fn write(self) {
        let content = match serde_json::to_vec_pretty(&self.value) {
            Ok(content) => content,
            Err(error) => {
                warn!(error = ?error, "failed to serialize the {} store", self.label);
                return;
            }
        };
        if let Err(error) = fsutil::write_file_atomic(&self.path, &content, FILE_MODE) {
            warn!(
                path = %self.path.display(),
                error = ?error,
                "failed to persist the {} store", self.label
            );
        }
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

    #[test]
    fn snapshots_round_trip_through_the_file() {
        let path = temp_path("round_trip");
        let mut store = JsonStore::<Counters>::load(path.clone(), "test");
        store.get_mut().visits = 7;

        store.take_snapshot(100, 0).unwrap().write();
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
    }
}
