use crate::api::types::UpdateStatusRequest;
use crate::api::ConnectorApiClient;
use crate::runtime::FacebookRuntime;

use super::phases::JobPhase;

pub async fn update_status(
    client: &ConnectorApiClient,
    scoped: &str,
    job_id: &str,
    phase: JobPhase,
    step_override: Option<&str>,
    runtime: Option<&FacebookRuntime>,
) -> Result<(), String> {
    let message = step_override
        .map(String::from)
        .unwrap_or_else(|| build_step_message(phase, runtime));

    client
        .update_status(
            UpdateStatusRequest {
                action: "update_status".into(),
                job_id: job_id.to_string(),
                status: phase.status_str().into(),
                progress: phase.progress(),
                current_step: message,
            },
            scoped,
        )
        .await
        .map(|_| ())
        .map_err(|e| format!("update_status failed for {job_id} at {}: {e}", phase.status_str()))
}

fn build_step_message(phase: JobPhase, runtime: Option<&FacebookRuntime>) -> String {
    let base = phase.default_message();
    let Some(rt) = runtime else {
        return base.to_string();
    };

    let status = rt.aggregate_status();
    match phase {
        JobPhase::StartingRuntime => format!("{base} (browser: {})", status.browser_state),
        JobPhase::CheckingFacebookSession => {
            format!("{base} (session: {})", status.facebook_session_state)
        }
        JobPhase::OpeningMarketplace => {
            format!("{base} (marketplace: {})", status.marketplace_state)
        }
        JobPhase::OpeningVehicleCreate | JobPhase::VerifyingVehicleCreate => {
            let dest = status
                .current_destination
                .as_deref()
                .unwrap_or("unknown");
            format!("{base} (destination: {dest})")
        }
        JobPhase::CreateRouteReady => {
            format!(
                "{base} — browser: {}, session: {}, marketplace: {}",
                status.browser_state, status.facebook_session_state, status.marketplace_state
            )
        }
        _ => base.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use crate::browser::{BrowserManager, BrowserRuntimeService, SidecarDaemon};
    use crate::runtime::FacebookRuntime;

    fn test_runtime() -> Arc<FacebookRuntime> {
        let runtime_svc = Arc::new(BrowserRuntimeService::new(false));
        let daemon = Arc::new(SidecarDaemon::new(std::path::PathBuf::new()));
        let manager = Arc::new(BrowserManager::new(runtime_svc, daemon));
        FacebookRuntime::new(manager)
    }

    #[test]
    fn step_message_without_runtime_uses_default() {
        let msg = build_step_message(JobPhase::OpeningMarketplace, None);
        assert_eq!(msg, JobPhase::OpeningMarketplace.default_message());
    }

    #[test]
    fn step_message_embeds_runtime_context() {
        let rt = test_runtime();
        let msg = build_step_message(JobPhase::CheckingFacebookSession, Some(&rt));
        assert!(msg.contains("session:"));
    }
}
