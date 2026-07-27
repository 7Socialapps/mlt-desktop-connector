use super::phases::JobPhase;

/// Structured job error codes for prepare-for-review automation (M3.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobErrorCode {
    PayloadValidationFailed,
    ImageDownloadFailed,
    BrowserNotReady,
    FacebookLoggedOut,
    FacebookCheckpoint,
    FacebookMfaRequired,
    FacebookSessionExpired,
    FacebookAccountRestricted,
    FacebookAccountDisabled,
    FacebookSessionUnknown,
    MarketplaceNavFailed,
    MarketplaceNotReady,
    VehicleCreateRouteNotReady,
    VehicleCreateVerificationFailed,
    ImageUploadFailed,
    FormFillFailed,
    FormVerificationFailed,
    OperationCancelled,
    RuntimeError,
}

impl JobErrorCode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::PayloadValidationFailed => "PAYLOAD_VALIDATION_FAILED",
            Self::ImageDownloadFailed => "IMAGE_DOWNLOAD_FAILED",
            Self::BrowserNotReady => "BROWSER_NOT_READY",
            Self::FacebookLoggedOut => "FACEBOOK_LOGGED_OUT",
            Self::FacebookCheckpoint => "FACEBOOK_CHECKPOINT",
            Self::FacebookMfaRequired => "FACEBOOK_MFA_REQUIRED",
            Self::FacebookSessionExpired => "FACEBOOK_SESSION_EXPIRED",
            Self::FacebookAccountRestricted => "FACEBOOK_ACCOUNT_RESTRICTED",
            Self::FacebookAccountDisabled => "FACEBOOK_ACCOUNT_DISABLED",
            Self::FacebookSessionUnknown => "FACEBOOK_SESSION_UNKNOWN",
            Self::MarketplaceNavFailed => "MARKETPLACE_NAV_FAILED",
            Self::MarketplaceNotReady => "MARKETPLACE_NOT_READY",
            Self::VehicleCreateRouteNotReady => "VEHICLE_CREATE_ROUTE_NOT_READY",
            Self::VehicleCreateVerificationFailed => "VEHICLE_CREATE_VERIFICATION_FAILED",
            Self::ImageUploadFailed => "IMAGE_UPLOAD_FAILED",
            Self::FormFillFailed => "FORM_FILL_FAILED",
            Self::FormVerificationFailed => "FORM_VERIFICATION_FAILED",
            Self::OperationCancelled => "OPERATION_CANCELLED",
            Self::RuntimeError => "RUNTIME_ERROR",
        }
    }

    pub fn default_message(&self) -> &'static str {
        match self {
            Self::PayloadValidationFailed => "Payload validation failed",
            Self::ImageDownloadFailed => "Listing photo download failed",
            Self::BrowserNotReady => "Browser is not ready for automation",
            Self::FacebookLoggedOut => "Facebook session is not signed in",
            Self::FacebookCheckpoint => "Facebook security checkpoint requires manual action",
            Self::FacebookMfaRequired => "Facebook MFA requires manual action",
            Self::FacebookSessionExpired => "Facebook session expired — sign in again",
            Self::FacebookAccountRestricted => "Facebook account is temporarily restricted",
            Self::FacebookAccountDisabled => "Facebook account is disabled",
            Self::FacebookSessionUnknown => "Facebook session state could not be determined",
            Self::MarketplaceNavFailed => "Marketplace navigation failed",
            Self::MarketplaceNotReady => "Marketplace is not ready",
            Self::VehicleCreateRouteNotReady => "Vehicle create route is not ready",
            Self::VehicleCreateVerificationFailed => {
                "Vehicle create form verification failed"
            }
            Self::ImageUploadFailed => "Listing photo upload failed",
            Self::FormFillFailed => "Listing field fill failed",
            Self::FormVerificationFailed => "Filled form verification failed",
            Self::OperationCancelled => "Operation cancelled by user",
            Self::RuntimeError => "Runtime error during job execution",
        }
    }

    pub fn failed_phase(&self) -> JobPhase {
        if *self == Self::OperationCancelled {
            JobPhase::Cancelled
        } else {
            JobPhase::Failed
        }
    }
}

#[derive(Debug)]
pub struct JobExecutionError {
    pub code: JobErrorCode,
    pub message: String,
    pub phase: JobPhase,
    pub screenshot_path: Option<String>,
}

impl JobExecutionError {
    pub fn new(code: JobErrorCode, message: impl Into<String>, phase: JobPhase) -> Self {
        Self {
            code,
            message: message.into(),
            phase,
            screenshot_path: None,
        }
    }

    pub fn cancelled() -> Self {
        Self {
            code: JobErrorCode::OperationCancelled,
            message: JobErrorCode::OperationCancelled.default_message().into(),
            phase: JobPhase::Cancelled,
            screenshot_path: None,
        }
    }

    pub fn with_screenshot(mut self, path: Option<String>) -> Self {
        self.screenshot_path = path;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_codes_are_upper_snake_case() {
        assert_eq!(
            JobErrorCode::VehicleCreateRouteNotReady.as_str(),
            "VEHICLE_CREATE_ROUTE_NOT_READY"
        );
        assert_eq!(
            JobErrorCode::OperationCancelled.as_str(),
            "OPERATION_CANCELLED"
        );
    }

    #[test]
    fn cancellation_maps_to_cancelled_phase() {
        assert_eq!(
            JobErrorCode::OperationCancelled.failed_phase(),
            JobPhase::Cancelled
        );
        assert_eq!(
            JobErrorCode::BrowserNotReady.failed_phase(),
            JobPhase::Failed
        );
    }
}
