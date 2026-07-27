use crate::browser::FacebookSessionState;

use super::errors::JobErrorCode;

/// Maps Facebook session states to job error codes before Marketplace navigation.
pub fn session_error_code(state: &FacebookSessionState) -> Option<JobErrorCode> {
    match state {
        FacebookSessionState::FacebookLoggedIn => None,
        FacebookSessionState::FacebookLoggedOut
        | FacebookSessionState::FacebookLoginInProgress => Some(JobErrorCode::FacebookLoggedOut),
        FacebookSessionState::FacebookCheckpoint => Some(JobErrorCode::FacebookCheckpoint),
        FacebookSessionState::FacebookMfaRequired => Some(JobErrorCode::FacebookMfaRequired),
        FacebookSessionState::FacebookSessionExpired => Some(JobErrorCode::FacebookSessionExpired),
        FacebookSessionState::FacebookTemporaryRestriction => {
            Some(JobErrorCode::FacebookAccountRestricted)
        }
        FacebookSessionState::FacebookDisabledAccount => {
            Some(JobErrorCode::FacebookAccountDisabled)
        }
        FacebookSessionState::FacebookNotChecked
        | FacebookSessionState::FacebookError => Some(JobErrorCode::FacebookSessionUnknown),
    }
}

pub fn session_error_message(code: &JobErrorCode, reason_code: Option<&str>) -> String {
    let base = code.default_message();
    match reason_code.filter(|r| !r.is_empty()) {
        Some(reason) => format!("{base} ({reason})"),
        None => base.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logged_in_passes_precheck() {
        assert!(session_error_code(&FacebookSessionState::FacebookLoggedIn).is_none());
    }

    #[test]
    fn checkpoint_maps_to_checkpoint_code() {
        assert_eq!(
            session_error_code(&FacebookSessionState::FacebookCheckpoint),
            Some(JobErrorCode::FacebookCheckpoint)
        );
    }

    #[test]
    fn logged_out_maps_to_logged_out_code() {
        assert_eq!(
            session_error_code(&FacebookSessionState::FacebookLoggedOut),
            Some(JobErrorCode::FacebookLoggedOut)
        );
    }

    #[test]
    fn not_checked_maps_to_unknown() {
        assert_eq!(
            session_error_code(&FacebookSessionState::FacebookNotChecked),
            Some(JobErrorCode::FacebookSessionUnknown)
        );
    }

    #[test]
    fn error_message_includes_reason_code() {
        let msg = session_error_message(&JobErrorCode::FacebookCheckpoint, Some("checkpoint_url"));
        assert!(msg.contains("checkpoint_url"));
    }
}
