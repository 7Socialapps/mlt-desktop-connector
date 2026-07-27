use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

use anyhow::{Context, Result};

use super::permissions::restrict_to_current_user;

fn write_sync(path: &Path, data: &[u8]) -> Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)
        .with_context(|| format!("failed to open {}", path.display()))?;
    file.write_all(data)
        .with_context(|| format!("failed to write {}", path.display()))?;
    file.sync_all()
        .with_context(|| format!("failed to fsync {}", path.display()))?;
    restrict_to_current_user(path)?;
    Ok(())
}

/// Atomically replace `target` with `data` via `temp` (no backup rotation).
pub(super) fn atomic_replace(target: &Path, temp: &Path, data: &[u8]) -> Result<()> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    write_sync(temp, data)?;
    replace_file(temp, target)?;

    if temp.exists() {
        let _ = fs::remove_file(temp);
    }

    Ok(())
}

fn replace_file(from: &Path, to: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        fs::rename(from, to).with_context(|| {
            format!("failed to rename {} to {}", from.display(), to.display())
        })?;
    }

    #[cfg(windows)]
    {
        if to.exists() {
            fs::remove_file(to)
                .with_context(|| format!("failed to replace {}", to.display()))?;
        }
        fs::rename(from, to).with_context(|| {
            format!("failed to rename {} to {}", from.display(), to.display())
        })?;
    }

    restrict_to_current_user(to)?;
    Ok(())
}

/// Write `data` atomically to `target` with a single rotating backup at `backup`.
///
/// 1. Write complete payload to `temp`
/// 2. `fsync()` the temp file
/// 3. Rotate existing `target` → `backup` (one generation only)
/// 4. Rename `temp` → `target`
pub fn atomic_write_with_backup(
    target: &Path,
    backup: &Path,
    temp: &Path,
    data: &[u8],
) -> Result<()> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    write_sync(temp, data)?;

    if target.exists() {
        if backup.exists() {
            fs::remove_file(backup)
                .with_context(|| format!("failed to remove old backup {}", backup.display()))?;
        }
        fs::rename(target, backup).with_context(|| {
            format!(
                "failed to rotate {} to backup {}",
                target.display(),
                backup.display()
            )
        })?;
        restrict_to_current_user(backup)?;
    }

    replace_file(temp, target)?;

    if temp.exists() {
        let _ = fs::remove_file(temp);
    }

    Ok(())
}

pub fn delete_credential_files(primary: &Path, backup: &Path, temp: &Path, restore_temp: &Path) -> Result<()> {
    for path in [primary, backup, temp, restore_temp] {
        if path.exists() {
            fs::remove_file(path)
                .with_context(|| format!("failed to delete {}", path.display()))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn paths(dir: &Path) -> (std::path::PathBuf, std::path::PathBuf, std::path::PathBuf) {
        (
            dir.join("credentials.enc"),
            dir.join("credentials.enc.bak"),
            dir.join("credentials.enc.tmp"),
        )
    }

    #[test]
    fn atomic_write_rotates_previous_primary_to_backup() {
        let dir = TempDir::new().expect("tempdir");
        let (target, backup, temp) = paths(dir.path());

        atomic_write_with_backup(&target, &backup, &temp, b"v1").expect("first write");
        assert!(target.exists());
        assert!(!backup.exists());

        atomic_write_with_backup(&target, &backup, &temp, b"v2").expect("second write");
        assert_eq!(fs::read(&target).expect("read primary"), b"v2");
        assert_eq!(fs::read(&backup).expect("read backup"), b"v1");
    }

    #[test]
    fn interrupted_temp_write_leaves_primary_intact() {
        let dir = TempDir::new().expect("tempdir");
        let (target, backup, temp) = paths(dir.path());

        atomic_write_with_backup(&target, &backup, &temp, b"stable").expect("write");
        fs::write(&temp, b"partial").expect("simulate crash mid-write");

        assert_eq!(fs::read(&target).expect("primary intact"), b"stable");
        assert!(fs::read(&temp).is_ok());
    }
}
