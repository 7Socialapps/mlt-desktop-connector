use serde::{Deserialize, Serialize};

/// Job lifecycle phases for prepare-for-review automation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobPhase {
    Queued,
    Claimed,
    ValidatingPayload,
    PreparingAssets,
    StartingRuntime,
    CheckingFacebookSession,
    OpeningMarketplace,
    OpeningVehicleCreate,
    VerifyingVehicleCreate,
    UploadingImages,
    FillingFields,
    VerifyingFields,
    ReadyForReview,
    Cancelled,
    Failed,
}

impl JobPhase {
    pub fn status_str(&self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Claimed => "claimed",
            Self::ValidatingPayload => "validating_payload",
            Self::PreparingAssets => "preparing_assets",
            Self::StartingRuntime => "starting_runtime",
            Self::CheckingFacebookSession => "checking_facebook_session",
            Self::OpeningMarketplace => "opening_marketplace",
            Self::OpeningVehicleCreate => "opening_vehicle_create",
            Self::VerifyingVehicleCreate => "verifying_vehicle_create",
            Self::UploadingImages => "images_uploading",
            Self::FillingFields => "fields_filling",
            Self::VerifyingFields => "verifying_fields",
            Self::ReadyForReview => "ready_for_review",
            Self::Cancelled => "cancelled",
            Self::Failed => "failed",
        }
    }

    pub fn progress(&self) -> u8 {
        match self {
            Self::Queued => 0,
            Self::Claimed => 5,
            Self::ValidatingPayload => 10,
            Self::PreparingAssets => 20,
            Self::StartingRuntime => 30,
            Self::CheckingFacebookSession => 40,
            Self::OpeningMarketplace => 50,
            Self::OpeningVehicleCreate => 60,
            Self::VerifyingVehicleCreate => 70,
            Self::UploadingImages => 80,
            Self::FillingFields => 88,
            Self::VerifyingFields => 95,
            Self::ReadyForReview => 100,
            Self::Cancelled => 0,
            Self::Failed => 0,
        }
    }

    pub fn default_message(&self) -> &'static str {
        match self {
            Self::Queued => "Job queued",
            Self::Claimed => "Job claimed",
            Self::ValidatingPayload => "Validating listing payload",
            Self::PreparingAssets => "Downloading listing photos",
            Self::StartingRuntime => "Starting browser runtime",
            Self::CheckingFacebookSession => "Checking Facebook session",
            Self::OpeningMarketplace => "Opening Facebook Marketplace",
            Self::OpeningVehicleCreate => "Opening vehicle create form",
            Self::VerifyingVehicleCreate => "Verifying vehicle create form",
            Self::UploadingImages => "Uploading listing photos",
            Self::FillingFields => "Filling listing fields",
            Self::VerifyingFields => "Verifying filled form",
            Self::ReadyForReview => "Listing prepared — ready for dealer review",
            Self::Cancelled => "Operation cancelled",
            Self::Failed => "Job failed",
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::ReadyForReview | Self::Cancelled | Self::Failed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ready_for_review_is_terminal_at_100_percent() {
        assert_eq!(JobPhase::ReadyForReview.progress(), 100);
        assert!(JobPhase::ReadyForReview.is_terminal());
        assert_eq!(JobPhase::ReadyForReview.status_str(), "ready_for_review");
    }

    #[test]
    fn phase_status_strings_are_snake_case() {
        for phase in [
            JobPhase::OpeningMarketplace,
            JobPhase::UploadingImages,
            JobPhase::FillingFields,
            JobPhase::CheckingFacebookSession,
        ] {
            let s = phase.status_str();
            assert!(!s.contains(' '));
            assert_eq!(s, s.to_lowercase());
        }
    }

    #[test]
    fn backend_aligned_upload_and_fill_statuses() {
        assert_eq!(JobPhase::UploadingImages.status_str(), "images_uploading");
        assert_eq!(JobPhase::FillingFields.status_str(), "fields_filling");
    }
}
