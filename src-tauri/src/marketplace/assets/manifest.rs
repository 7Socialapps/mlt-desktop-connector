use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ManifestImageEntry {
    pub index: u32,
    pub source_url: String,
    pub local_path: String,
    pub sha256: String,
    pub mime: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JobAssetManifest {
    pub job_id: String,
    pub contract_version: u32,
    pub images: Vec<ManifestImageEntry>,
}

pub fn write_manifest(dir: &Path, manifest: &JobAssetManifest) -> Result<(), String> {
    let target = dir.join("manifest.json");
    let temp = dir.join("manifest.json.tmp");
    let json = serde_json::to_vec_pretty(manifest)
        .map_err(|e| format!("manifest serialization failed: {e}"))?;
    fs::write(&temp, &json).map_err(|e| format!("failed to write {}: {e}", temp.display()))?;
    fs::rename(&temp, &target).map_err(|e| format!("failed to rename manifest: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn write_manifest_creates_ordered_json_file() {
        let tmp = TempDir::new().unwrap();
        let manifest = JobAssetManifest {
            job_id: "job-1".into(),
            contract_version: 1,
            images: vec![ManifestImageEntry {
                index: 0,
                source_url: "https://cdn.example/a.jpg".into(),
                local_path: "001.jpg".into(),
                sha256: "abc".into(),
                mime: "image/jpeg".into(),
                bytes: 10,
            }],
        };
        write_manifest(tmp.path(), &manifest).unwrap();
        let raw = fs::read_to_string(tmp.path().join("manifest.json")).unwrap();
        assert!(raw.contains("001.jpg"));
    }
}
