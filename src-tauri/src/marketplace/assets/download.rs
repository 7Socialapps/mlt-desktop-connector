use std::collections::HashSet;
use std::fs;
use std::path::Path;

use reqwest::Client;
use sha2::{Digest, Sha256};
use tauri::AppHandle;
use tracing::{info, warn};

use super::dedupe::dedupe_image_urls;
use super::manifest::{JobAssetManifest, ManifestImageEntry};
use super::validate::{extension_for_mime, validate_image_bytes};
use super::workspace::JobAssetWorkspace;
use crate::marketplace::payload::VehicleJobPayload;

const CONTRACT_VERSION: u32 = 1;

#[derive(Debug)]
pub enum AssetError {
    Download { index: usize, url: String, message: String },
    Validation { index: usize, message: String },
}

impl AssetError {
    pub fn user_message(&self) -> String {
        match self {
            Self::Download { index, url, message } => {
                format!("Failed to download photo {} from {url}: {message}", index + 1)
            }
            Self::Validation { index, message } => {
                format!("Photo {} failed validation: {message}", index + 1)
            }
        }
    }
}

pub async fn download_job_assets(
    app: &AppHandle,
    payload: &VehicleJobPayload,
) -> Result<(JobAssetManifest, JobAssetWorkspace), AssetError> {
    let workspace = JobAssetWorkspace::create(app, &payload.job_id).map_err(|e| AssetError::Download {
        index: 0,
        url: String::new(),
        message: e,
    })?;

    let urls = dedupe_image_urls(&payload.ordered_image_urls);
    if urls.is_empty() {
        return Err(AssetError::Validation {
            index: 0,
            message: "ordered_image_urls is empty after deduplication".into(),
        });
    }

    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| AssetError::Download {
            index: 0,
            url: String::new(),
            message: format!("HTTP client init failed: {e}"),
        })?;

    let mut images = Vec::new();
    let mut content_hashes = HashSet::new();

    for (index, url) in urls.iter().enumerate() {
        info!(job_id = %payload.job_id, index, url, "downloading listing photo");
        let response = client.get(url).send().await.map_err(|e| AssetError::Download {
            index,
            url: url.clone(),
            message: e.to_string(),
        })?;

        if !response.status().is_success() {
            return Err(AssetError::Download {
                index,
                url: url.clone(),
                message: format!("HTTP {}", response.status()),
            });
        }

        let bytes = response.bytes().await.map_err(|e| AssetError::Download {
            index,
            url: url.clone(),
            message: e.to_string(),
        })?;

        let mime = validate_image_bytes(&bytes).map_err(|message| AssetError::Validation {
            index,
            message,
        })?;

        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        let sha256 = format!("{:x}", hasher.finalize());
        if !content_hashes.insert(sha256.clone()) {
            warn!(job_id = %payload.job_id, index, url, "skipping duplicate image content");
            continue;
        }

        let ext = extension_for_mime(mime);
        let filename = format!("{:03}.{}", index + 1, ext);
        let file_path = workspace.path().join(&filename);
        fs::write(&file_path, &bytes).map_err(|e| AssetError::Download {
            index,
            url: url.clone(),
            message: format!("failed to write {}: {e}", file_path.display()),
        })?;

        images.push(ManifestImageEntry {
            index: images.len() as u32,
            source_url: url.clone(),
            local_path: filename,
            sha256,
            mime: mime.to_string(),
            bytes: bytes.len() as u64,
        });
    }

    if images.is_empty() {
        let _ = workspace.cleanup();
        return Err(AssetError::Validation {
            index: 0,
            message: "no unique valid images after download and deduplication".into(),
        });
    }

    let manifest = JobAssetManifest {
        job_id: payload.job_id.clone(),
        contract_version: CONTRACT_VERSION,
        images,
    };

    write_manifest_file(workspace.path(), &manifest).map_err(|message| AssetError::Validation {
        index: 0,
        message,
    })?;

    Ok((manifest, workspace))
}

fn write_manifest_file(dir: &Path, manifest: &JobAssetManifest) -> Result<(), String> {
    super::manifest::write_manifest(dir, manifest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::marketplace::payload::types::{ExecutionMode, ListingOptions};

    #[test]
    fn asset_error_messages_are_user_friendly() {
        let err = AssetError::Validation {
            index: 2,
            message: "unsupported or corrupt image data".into(),
        };
        assert!(err.user_message().contains("Photo 3"));
    }

    #[test]
    fn dedupe_integration_preserves_first_url() {
        let urls = dedupe_image_urls(&[
            "https://a/1.jpg".into(),
            "https://a/1.jpg".into(),
            "https://a/2.jpg".into(),
        ]);
        assert_eq!(urls.len(), 2);
    }

    #[allow(dead_code)]
    fn sample_payload(urls: Vec<String>) -> VehicleJobPayload {
        VehicleJobPayload {
            contract_version: 1,
            job_id: "job-assets".into(),
            user_id: String::new(),
            dealership_id: String::new(),
            inventory_id: String::new(),
            inventory_source: String::new(),
            year: String::new(),
            make: String::new(),
            model: String::new(),
            trim: String::new(),
            body_style: String::new(),
            vehicle_type: String::new(),
            condition: String::new(),
            price: String::new(),
            mileage: String::new(),
            vin: String::new(),
            stock_number: String::new(),
            exterior_color: String::new(),
            interior_color: String::new(),
            transmission: String::new(),
            drivetrain: String::new(),
            fuel_type: String::new(),
            title: String::new(),
            description: String::new(),
            location: "Austin, TX".into(),
            ordered_image_urls: urls,
            listing_options: ListingOptions::default(),
            posting_preferences: serde_json::json!({}),
            execution_mode: ExecutionMode::PrepareForReview,
            idempotency_key: String::new(),
            source_metadata: serde_json::json!({}),
            expires_at: String::new(),
            test: false,
            label: None,
            scoped_job_token: None,
        }
    }
}
