use anyhow::{Context, Result};
use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

pub(crate) fn write_file_atomic(path: &Path, data: &[u8], mode: u32) -> Result<()> {
    ensure_parent_dir(path)?;
    let mut tmp = path.to_path_buf();
    tmp.set_extension("tmp");

    let mut options = OpenOptions::new();
    options.create(true).truncate(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(mode);
    }

    let mut file = options
        .open(&tmp)
        .with_context(|| format!("failed to open {}", tmp.display()))?;
    file.write_all(data)
        .with_context(|| format!("failed to write {}", tmp.display()))?;
    file.sync_all()
        .with_context(|| format!("failed to sync {}", tmp.display()))?;
    fs::rename(&tmp, path)
        .with_context(|| format!("failed to move {} to {}", tmp.display(), path.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(mode))
            .with_context(|| format!("failed to set permissions on {}", path.display()))?;
    }

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
