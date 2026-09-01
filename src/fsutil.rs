use anyhow::{Context, Result};
use std::fs;
use std::fs::File;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Splits a base state path into one file per chain:
/// `state.json` plus `everscale` becomes `state_everscale.json`.
pub(crate) fn chain_file_path(base_path: &Path, chain_id: &str) -> PathBuf {
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
        .unwrap_or_else(|| format!("validatorclock_{safe_chain_id}.json"));

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

/// Moves a file whose contents no longer parse out of the way and reports
/// where it went. Writing fresh state over it would destroy data that only a
/// person can make sense of. Use this for a file that was read but could not
/// be understood - never for one that merely could not be read, which may be
/// perfectly good.
pub(crate) fn keep_unreadable_file(path: &Path) -> Result<PathBuf> {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .with_context(|| format!("{} has no file name", path.display()))?;
    let kept = path.with_file_name(format!("{name}.unreadable-{}", crate::timeutil::now_sec()));

    fs::rename(path, &kept).with_context(|| format!("failed to move {} aside", path.display()))?;
    Ok(kept)
}

pub(crate) fn write_file_atomic(path: &Path, data: &[u8], mode: u32) -> Result<()> {
    ensure_parent_dir(path)?;
    let tmp = temp_file_path(path);
    let result = write_file_atomic_inner(path, &tmp, data, mode);
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

fn write_file_atomic_inner(path: &Path, tmp: &Path, data: &[u8], mode: u32) -> Result<()> {
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(mode);
    }

    let mut file = options
        .open(tmp)
        .with_context(|| format!("failed to open {}", tmp.display()))?;
    file.write_all(data)
        .with_context(|| format!("failed to write {}", tmp.display()))?;
    file.sync_all()
        .with_context(|| format!("failed to sync {}", tmp.display()))?;
    fs::rename(tmp, path)
        .with_context(|| format!("failed to move {} to {}", tmp.display(), path.display()))?;
    sync_parent_dir(path)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(mode))
            .with_context(|| format!("failed to set permissions on {}", path.display()))?;
    }

    Ok(())
}

fn temp_file_path(path: &Path) -> PathBuf {
    let counter = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("validatorclock");
    path.with_file_name(format!(
        ".{file_name}.{}.{}.{}.tmp",
        std::process::id(),
        nanos,
        counter
    ))
}

#[cfg(unix)]
fn sync_parent_dir(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        File::open(parent)
            .with_context(|| format!("failed to open {}", parent.display()))?
            .sync_all()
            .with_context(|| format!("failed to sync {}", parent.display()))?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn sync_parent_dir(_: &Path) -> Result<()> {
    Ok(())
}

fn ensure_parent_dir(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    Ok(())
}
