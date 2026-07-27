use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FieldFillResult {
    pub field: String,
    pub ok: bool,
    #[serde(default)]
    pub expected: Option<String>,
    #[serde(default)]
    pub actual: Option<String>,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub optional: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FormFillReport {
    pub selector_version: String,
    pub fields: Vec<FieldFillResult>,
    pub filled: Vec<String>,
    pub failed: Vec<String>,
    pub skipped: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ImageUploadResult {
    pub index: u32,
    pub ok: bool,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub attempts: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ImageUploadReport {
    pub uploaded: Vec<ImageUploadResult>,
    pub thumbnail_count: u32,
    pub expected_count: u32,
    pub primary_preserved: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct FormVerificationReport {
    pub ready: bool,
    pub reason_code: String,
    #[serde(default)]
    pub fields_ok: Vec<String>,
    #[serde(default)]
    pub fields_missing: Vec<String>,
    #[serde(default)]
    pub fields_mismatch: Vec<String>,
    #[serde(default)]
    pub image_count: u32,
    #[serde(default)]
    pub expected_image_count: u32,
    #[serde(default)]
    pub has_validation_errors: bool,
    #[serde(default)]
    pub has_next_button: bool,
    #[serde(default)]
    pub has_publish_button: bool,
    #[serde(default)]
    pub screenshot_path: Option<String>,
    #[serde(default)]
    pub current_url: Option<String>,
    #[serde(default)]
    pub checked_at: Option<String>,
}

impl FormFillReport {
    pub fn required_failures(&self) -> Vec<String> {
        self.fields
            .iter()
            .filter(|f| !f.ok && !f.optional && f.reason.as_deref() != Some("empty_skipped"))
            .map(|f| f.field.clone())
            .collect()
    }
}

impl ImageUploadReport {
    pub fn all_uploaded(&self) -> bool {
        self.uploaded.iter().all(|u| u.ok)
    }
}
