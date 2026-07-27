use serde::{Deserialize, Serialize};

/// Job lifecycle phases for prepare-for-review automation (M3.3).
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
    CreateRouteReady,
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
            Self::CreateRouteReady => "create_route_ready",
            Self::Cancelled => "cancelled",
            Self::Failed => "failed",
        }
    }

    pub fn progress(&self) -> u8 {
        match self {
            Self::Queued => 0,
            Self::Claimed => 5,
            Self::ValidatingPayload => 10,
            Self::PreparingAssets => 30,
            Self::StartingRuntime => 40,
            Self::CheckingFacebookSession => 50,
            Self::OpeningMarketplace => 60,
            Self::OpeningVehicleCreate => 75,
            Self::VerifyingVehicleCreate => 85,
            Self::CreateRouteReady => 100,
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
            Self::CreateRouteReady => "Vehicle create route ready for review",
            Self::Cancelled => "Operation cancelled",
            Self::Failed => "Job failed",
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::CreateRouteReady | Self::Cancelled | Self::Failed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_route_ready_is_terminal_at_100_percent() {
        assert_eq!(JobPhase::CreateRouteReady.progress(), 100);
        assert!(JobPhase::CreateRouteReady.is_terminal());
        assert_eq!(
            JobPhase::CreateRouteReady.status_str(),
            "create_route_ready"
        );
    }

    #[test]
    fn phase_status_strings_are_snake_case() {
        for phase in [
            JobPhase::OpeningMarketplace,
            JobPhase::VerifyingVehicleCreate,
            JobPhase::CheckingFacebookSession,
        ] {
            let s = phase.status_str();
            assert!(!s.contains(' '));
            assert_eq!(s, s.to_lowercase());
        }
    }
}
