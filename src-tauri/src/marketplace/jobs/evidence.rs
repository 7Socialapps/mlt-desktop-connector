use std::fs;
use std::path::{Path, PathBuf};

use tauri::AppHandle;
use tauri::Manager;

/// Copies a diagnostic screenshot into local job evidence storage (no secrets).
pub fn store_job_screenshot(
    app: &AppHandle,
    job_id: &str,
    source_path: &str,
    label: &str,
) -> Result<PathBuf, String> {
    let source = Path::new(source_path);
    if !source.exists() {
        return Err(format!("evidence source not found: {source_path}"));
    }

    let evidence_dir = job_evidence_dir(app, job_id)?;
    fs::create_dir_all(&evidence_dir)
        .map_err(|e| format!("failed to create evidence dir: {e}"))?;

    let safe_label: String = label
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect();
    let filename = format!("{safe_label}-{}.png", chrono::Utc::now().timestamp_millis());
    let dest = evidence_dir.join(filename);

    fs::copy(source, &dest).map_err(|e| format!("failed to copy evidence screenshot: {e}"))?;
    Ok(dest)
}

pub fn job_evidence_dir(app: &AppHandle, job_id: &str) -> Result<PathBuf, String> {
    let base = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("app data dir unavailable: {e}"))?;
    Ok(base.join("job-evidence").join(job_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_label_strips_unsafe_chars() {
        let label: String = "verify failed!"
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
            .collect();
        assert_eq!(label, "verify_failed_");
    }
}
