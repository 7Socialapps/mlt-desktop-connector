use std::fs;
use std::path::{Path, PathBuf};

use tauri::{AppHandle, Manager};
use tracing::info;

const JOB_ASSETS_DIR: &str = "job-assets";

pub fn resolve_job_assets_dir(app: &AppHandle, job_id: &str) -> Result<PathBuf, String> {
    let base = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("app_data_dir unavailable: {e}"))?;
    Ok(base.join(JOB_ASSETS_DIR).join(job_id))
}

pub struct JobAssetWorkspace {
    path: PathBuf,
    cleaned: bool,
}

impl JobAssetWorkspace {
    pub fn create(app: &AppHandle, job_id: &str) -> Result<Self, String> {
        let path = resolve_job_assets_dir(app, job_id)?;
        fs::create_dir_all(&path).map_err(|e| format!("failed to create {}: {e}", path.display()))?;
        info!(job_id, dir = %path.display(), "created job asset workspace");
        Ok(Self { path, cleaned: false })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn cleanup(mut self) -> Result<(), String> {
        self.cleaned = true;
        if self.path.exists() {
            fs::remove_dir_all(&self.path)
                .map_err(|e| format!("failed to remove {}: {e}", self.path.display()))?;
            info!(dir = %self.path.display(), "removed job asset workspace");
        }
        Ok(())
    }
}

impl Drop for JobAssetWorkspace {
    fn drop(&mut self) {
        if !self.cleaned && self.path.exists() {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn cleanup_removes_workspace_directory() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("job-assets").join("job-1");
        fs::create_dir_all(&path).unwrap();
        fs::write(path.join("001.jpg"), b"fake").unwrap();

        let workspace = JobAssetWorkspace {
            path: path.clone(),
            cleaned: false,
        };
        workspace.cleanup().unwrap();
        assert!(!path.exists());
    }
}
