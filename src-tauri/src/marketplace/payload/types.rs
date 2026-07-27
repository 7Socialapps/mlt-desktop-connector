use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionMode {
    TransportTest,
    PrepareForReview,
    PublishAfterReview,
    FullyAutomatic,
}

impl ExecutionMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::TransportTest => "transport_test",
            Self::PrepareForReview => "prepare_for_review",
            Self::PublishAfterReview => "publish_after_review",
            Self::FullyAutomatic => "fully_automatic",
        }
    }

    pub fn is_supported_in_m3(&self) -> bool {
        matches!(self, Self::TransportTest | Self::PrepareForReview)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ListingOptions {
    #[serde(default)]
    pub vehicle_category: String,
    #[serde(default)]
    pub emoji: String,
    #[serde(default)]
    pub distance_unit: String,
    #[serde(default)]
    pub posting_destination: String,
    #[serde(default)]
    pub language: String,
    #[serde(default)]
    pub contact_name: String,
    #[serde(default)]
    pub contact_phone: String,
    #[serde(default)]
    pub include_mileage: bool,
    #[serde(default)]
    pub ftc_safe_mode: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct VehicleJobPayload {
    pub contract_version: u32,
    pub job_id: String,
    pub user_id: String,
    pub dealership_id: String,
    pub inventory_id: String,
    pub inventory_source: String,
    pub year: String,
    pub make: String,
    pub model: String,
    #[serde(default)]
    pub trim: String,
    pub body_style: String,
    pub vehicle_type: String,
    pub condition: String,
    pub price: String,
    pub mileage: String,
    pub vin: String,
    #[serde(default)]
    pub stock_number: String,
    #[serde(default)]
    pub exterior_color: String,
    #[serde(default)]
    pub interior_color: String,
    pub transmission: String,
    #[serde(default)]
    pub drivetrain: String,
    pub fuel_type: String,
    pub title: String,
    pub description: String,
    pub location: String,
    pub ordered_image_urls: Vec<String>,
    pub listing_options: ListingOptions,
    #[serde(default)]
    pub posting_preferences: Value,
    pub execution_mode: ExecutionMode,
    pub idempotency_key: String,
    #[serde(default)]
    pub source_metadata: Value,
    pub expires_at: String,
    #[serde(default)]
    pub test: bool,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub scoped_job_token: Option<String>,
}

impl VehicleJobPayload {
    pub fn is_transport_test(&self) -> bool {
        self.test || matches!(self.execution_mode, ExecutionMode::TransportTest)
    }
}
