use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("failed to read launch session store: {0}")]
    Read(String),
    #[error("failed to write launch session store: {0}")]
    Write(String),
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct StoreFile {
    redeemed: HashSet<String>,
}

#[derive(Debug, Clone)]
pub struct LaunchSessionStore {
    path: PathBuf,
}

impl LaunchSessionStore {
    pub fn at_path(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn load(path: &Path) -> Self {
        Self {
            path: path.to_path_buf(),
        }
    }

    pub fn is_redeemed(&self, session_id: &str) -> bool {
        self.read_file()
            .map(|file| file.redeemed.contains(session_id))
            .unwrap_or(false)
    }

    pub fn mark_redeemed(&self, session_id: &str) -> Result<(), StoreError> {
        let mut file = self.read_file().unwrap_or_default();
        file.redeemed.insert(session_id.to_string());
        self.write_file(&file)
    }

    fn read_file(&self) -> Result<StoreFile, StoreError> {
        if !self.path.exists() {
            return Ok(StoreFile::default());
        }
        let raw = fs::read_to_string(&self.path).map_err(|e| StoreError::Read(e.to_string()))?;
        serde_json::from_str(&raw).map_err(|e| StoreError::Read(e.to_string()))
    }

    fn write_file(&self, file: &StoreFile) -> Result<(), StoreError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|e| StoreError::Write(e.to_string()))?;
        }
        let raw = serde_json::to_string_pretty(file).map_err(|e| StoreError::Write(e.to_string()))?;
        fs::write(&self.path, raw).map_err(|e| StoreError::Write(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn persists_redeemed_ids() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("redeemed.json");
        let store = LaunchSessionStore::at_path(path.clone());
        store.mark_redeemed("abc").expect("mark");
        let reloaded = LaunchSessionStore::at_path(path);
        assert!(reloaded.is_redeemed("abc"));
        assert!(!reloaded.is_redeemed("other"));
    }
}
