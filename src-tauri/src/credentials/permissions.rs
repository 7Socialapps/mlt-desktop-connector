use std::path::Path;

use anyhow::{Context, Result};

/// Restrict credential files to the current OS user.
pub fn restrict_to_current_user(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
            .with_context(|| format!("failed to set 0600 permissions on {}", path.display()))?;
    }

    #[cfg(windows)]
    {
        restrict_windows_user_acl(path)?;
    }

    Ok(())
}

#[cfg(windows)]
fn restrict_windows_user_acl(path: &Path) -> Result<()> {
    use std::process::Command;

    let username = std::env::var("USERNAME")
        .or_else(|_| std::env::var("USER"))
        .context("failed to resolve current username for ACL")?;
    let path_str = path.to_string_lossy();

    // Remove inherited ACLs; grant read/write to the current user only.
    let status = Command::new("icacls")
        .args([
            path_str.as_ref(),
            "/inheritance:r",
            &format!("{username}:(R,W)"),
            "/grant:r",
            &format!("{username}:(R,W)"),
        ])
        .status()
        .context("failed to run icacls for credential file ACL")?;

    if !status.success() {
        anyhow::bail!("icacls returned non-zero exit code for {}", path.display());
    }

    Ok(())
}
