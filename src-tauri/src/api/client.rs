use std::time::Duration;

use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde::de::DeserializeOwned;
use thiserror::Error;
use tracing::warn;

use crate::config::AppConfig;
use crate::marketplace::payload::VehicleJobPayload;

use super::types::*;

const FUNCTION_NAME: &str = "browser-connector";
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Error)]
pub enum ApiClientError {
    #[error("HTTP {status}: {message}")]
    Http { status: u16, message: String, code: Option<String> },
    #[error("network error: {0}")]
    Network(#[from] reqwest::Error),
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

pub struct AuthHeaders {
    pub device_access_token: Option<String>,
    pub neon_session_token: Option<String>,
    pub pairing_session_id: Option<String>,
    pub pairing_session_secret: Option<String>,
    pub scoped_job_token: Option<String>,
}

pub struct ConnectorApiClient {
    http: reqwest::Client,
    config: AppConfig,
}

/// Unwrap optional `{ data: ... }` wrappers used by some dashboard clients.
/// Direct edge-function calls return a flat JSON body.
fn unwrap_response_payload(value: serde_json::Value) -> serde_json::Value {
    if value.get("ok").is_none()
        && value.get("error").is_none()
        && value.get("data").is_some()
    {
        value.get("data").cloned().unwrap_or(value)
    } else {
        value
    }
}

fn parse_error_body(status: u16, text: &str) -> ApiClientError {
    if let Ok(err) = serde_json::from_str::<ApiErrorBody>(text) {
        return ApiClientError::Http {
            status,
            message: err.error.unwrap_or_else(|| text.to_string()),
            code: err.error_code,
        };
    }
    ApiClientError::Http {
        status,
        message: text.to_string(),
        code: None,
    }
}

fn parse_success_body<T: DeserializeOwned>(text: &str) -> Result<T, ApiClientError> {
    let value: serde_json::Value = serde_json::from_str(text)?;
    let payload = unwrap_response_payload(value);
    serde_json::from_value(payload).map_err(ApiClientError::from)
}

impl ConnectorApiClient {
    pub fn new(config: AppConfig) -> Self {
        let http = reqwest::Client::builder()
            .timeout(DEFAULT_TIMEOUT)
            .build()
            .expect("failed to build HTTP client");

        Self { http, config }
    }

    fn endpoint(&self) -> String {
        format!(
            "{}/functions/v1/{}",
            self.config.supabase_url, FUNCTION_NAME
        )
    }

    fn base_headers(&self, auth: &AuthHeaders) -> Result<HeaderMap, ApiClientError> {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert(
            "apikey",
            HeaderValue::from_str(&self.config.supabase_anon_key)
                .map_err(|e| ApiClientError::Http {
                    status: 0,
                    message: e.to_string(),
                    code: None,
                })?,
        );
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", self.config.supabase_anon_key))
                .map_err(|e| ApiClientError::Http {
                    status: 0,
                    message: e.to_string(),
                    code: None,
                })?,
        );

        if let Some(token) = &auth.device_access_token {
            headers.insert(
                "x-connector-device-token",
                HeaderValue::from_str(token).map_err(|e| ApiClientError::Http {
                    status: 0,
                    message: e.to_string(),
                    code: None,
                })?,
            );
        }
        if let Some(token) = &auth.neon_session_token {
            headers.insert(
                "x-neon-session-token",
                HeaderValue::from_str(token).map_err(|e| ApiClientError::Http {
                    status: 0,
                    message: e.to_string(),
                    code: None,
                })?,
            );
        }
        if let Some(session) = &auth.pairing_session_id {
            headers.insert(
                "x-connector-pairing-session",
                HeaderValue::from_str(session).map_err(|e| ApiClientError::Http {
                    status: 0,
                    message: e.to_string(),
                    code: None,
                })?,
            );
        }
        if let Some(secret) = &auth.pairing_session_secret {
            headers.insert(
                "x-connector-pairing-secret",
                HeaderValue::from_str(secret).map_err(|e| ApiClientError::Http {
                    status: 0,
                    message: e.to_string(),
                    code: None,
                })?,
            );
        }
        if let Some(job_token) = &auth.scoped_job_token {
            headers.insert(
                "x-connector-job-token",
                HeaderValue::from_str(job_token).map_err(|e| ApiClientError::Http {
                    status: 0,
                    message: e.to_string(),
                    code: None,
                })?,
            );
        }

        Ok(headers)
    }

    async fn post<T: DeserializeOwned>(
        &self,
        body: &impl serde::Serialize,
        auth: &AuthHeaders,
    ) -> Result<T, ApiClientError> {
        let response = self
            .http
            .post(self.endpoint())
            .headers(self.base_headers(auth)?)
            .json(body)
            .send()
            .await?;

        let status = response.status();
        let text = response.text().await?;

        if !status.is_success() {
            warn!(
                http_status = status.as_u16(),
                body = %text,
                "connector API error response"
            );
            return Err(parse_error_body(status.as_u16(), &text));
        }

        parse_success_body(&text)
    }

    pub async fn register_device(
        &self,
        request: RegisterDeviceRequest,
        neon_session_token: &str,
    ) -> Result<RegisterDeviceResponse, ApiClientError> {
        self.post(
            &request,
            &AuthHeaders {
                device_access_token: None,
                neon_session_token: Some(neon_session_token.to_string()),
                ..AuthHeaders::default()
            },
        )
        .await
    }

    pub async fn authenticate_device(
        &self,
        refresh_token: &str,
    ) -> Result<AuthenticateDeviceResponse, ApiClientError> {
        let body = AuthenticateDeviceRequest {
            action: "authenticate_device".into(),
            refresh_token: refresh_token.to_string(),
        };
        self.post(&body, &AuthHeaders::default()).await
    }

    pub async fn heartbeat(
        &self,
        request: HeartbeatRequest,
        device_access_token: &str,
    ) -> Result<HeartbeatResponse, ApiClientError> {
        self.post(
            &request,
            &AuthHeaders {
                device_access_token: Some(device_access_token.to_string()),
                ..AuthHeaders::default()
            },
        )
        .await
    }

    pub async fn poll_jobs(
        &self,
        request: PollJobsRequest,
        device_access_token: &str,
    ) -> Result<PollJobsResponse, ApiClientError> {
        self.post(
            &request,
            &AuthHeaders {
                device_access_token: Some(device_access_token.to_string()),
                ..AuthHeaders::default()
            },
        )
        .await
    }

    pub async fn create_pairing_session(
        &self,
        request: CreatePairingSessionRequest,
    ) -> Result<CreatePairingSessionResponse, ApiClientError> {
        self.post(&request, &AuthHeaders::default()).await
    }

    pub async fn poll_pairing_session(
        &self,
        request: PollPairingSessionRequest,
    ) -> Result<PollPairingSessionResponse, ApiClientError> {
        self.post(
            &request,
            &AuthHeaders {
                pairing_session_id: Some(request.session_id.clone()),
                pairing_session_secret: Some(request.session_secret.clone()),
                ..AuthHeaders::default()
            },
        )
        .await
    }

    pub async fn claim_job(
        &self,
        request: ClaimJobRequest,
        device_access_token: &str,
    ) -> Result<ClaimJobResponse, ApiClientError> {
        self.post(
            &request,
            &AuthHeaders {
                device_access_token: Some(device_access_token.to_string()),
                ..AuthHeaders::default()
            },
        )
        .await
    }

    pub async fn get_payload(
        &self,
        request: GetPayloadRequest,
        scoped_job_token: &str,
    ) -> Result<VehicleJobPayload, ApiClientError> {
        self.post(
            &request,
            &AuthHeaders {
                scoped_job_token: Some(scoped_job_token.to_string()),
                ..AuthHeaders::default()
            },
        )
        .await
    }

    pub async fn fail_job(
        &self,
        request: FailJobRequest,
        scoped_job_token: &str,
    ) -> Result<serde_json::Value, ApiClientError> {
        self.post(
            &request,
            &AuthHeaders {
                scoped_job_token: Some(scoped_job_token.to_string()),
                ..AuthHeaders::default()
            },
        )
        .await
    }

    pub async fn update_status(
        &self,
        request: UpdateStatusRequest,
        scoped_job_token: &str,
    ) -> Result<serde_json::Value, ApiClientError> {
        self.post(
            &request,
            &AuthHeaders {
                scoped_job_token: Some(scoped_job_token.to_string()),
                ..AuthHeaders::default()
            },
        )
        .await
    }

    pub async fn complete_job(
        &self,
        request: CompleteJobRequest,
        scoped_job_token: &str,
    ) -> Result<serde_json::Value, ApiClientError> {
        self.post(
            &request,
            &AuthHeaders {
                scoped_job_token: Some(scoped_job_token.to_string()),
                ..AuthHeaders::default()
            },
        )
        .await
    }

    /// Legacy stub — prefer pairing session flow.
    pub async fn create_pairing_code(
        &self,
        request: CreatePairingCodeRequest,
    ) -> Result<CreatePairingCodeResponse, ApiClientError> {
        self.post(&request, &AuthHeaders::default()).await
    }

    /// Proposed pairing action — backend not yet deployed.
    pub async fn exchange_pairing_code(
        &self,
        request: ExchangePairingCodeRequest,
        neon_session_token: &str,
    ) -> Result<ExchangePairingCodeResponse, ApiClientError> {
        self.post(
            &request,
            &AuthHeaders {
                device_access_token: None,
                neon_session_token: Some(neon_session_token.to_string()),
                ..AuthHeaders::default()
            },
        )
        .await
    }

    pub async fn redeem_launch_session(
        &self,
        request: RedeemLaunchSessionRequest,
        device_access_token: &str,
    ) -> Result<RedeemLaunchSessionResponse, ApiClientError> {
        self.post(
            &request,
            &AuthHeaders {
                device_access_token: Some(device_access_token.to_string()),
                ..AuthHeaders::default()
            },
        )
        .await
    }

    pub async fn acknowledge_launch_session(
        &self,
        request: AcknowledgeLaunchSessionRequest,
        device_access_token: &str,
    ) -> Result<AcknowledgeLaunchSessionResponse, ApiClientError> {
        self.post(
            &request,
            &AuthHeaders {
                device_access_token: Some(device_access_token.to_string()),
                ..AuthHeaders::default()
            },
        )
        .await
    }
}

impl Default for AuthHeaders {
    fn default() -> Self {
        Self {
            device_access_token: None,
            neon_session_token: None,
            pairing_session_id: None,
            pairing_session_secret: None,
            scoped_job_token: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const POLL_PENDING: &str = r#"{"ok":true,"status":"pairing_pending","accessToken":null,"refreshToken":null,"accessExpiresIn":null,"refreshExpiresIn":null,"deviceId":null,"userId":null,"dealershipId":null}"#;

    const POLL_COMPLETED: &str = r#"{"ok":true,"status":"pairing_completed","accessToken":"access-test-token","refreshToken":"refresh-test-token","accessExpiresIn":3600,"refreshExpiresIn":2592000,"deviceId":"dev-abc","userId":"user-1","dealershipId":"dealer-1"}"#;

    const POLL_COMPLETED_NO_TOKENS: &str = r#"{"ok":true,"status":"pairing_completed","accessToken":null,"refreshToken":null,"accessExpiresIn":null,"refreshExpiresIn":null,"deviceId":"dev-abc","userId":"user-1","dealershipId":"dealer-1"}"#;

    const POLL_ERROR: &str =
        r#"{"error":"Pairing session not found","errorCode":"PAIRING_NOT_FOUND"}"#;

    const CLAIM_SUCCESS: &str = r#"{"ok":true,"jobId":"job-123","status":"job_claimed","scopedJobToken":"scoped.jwt.token","tokenExpiresInSeconds":1800}"#;

    const UPDATE_STATUS_SUCCESS: &str =
        r#"{"ok":true,"jobId":"job-123","status":"browser_opening","progress":10}"#;

    const COMPLETE_TEST_JOB: &str = r#"{"ok":true,"jobId":"job-123","status":"posted","listingUrl":"https://www.facebook.com/marketplace/item/9999999999","test":true}"#;

    const REDEEM_SUCCESS: &str =
        r#"{"ok":true,"nonce":"nonce-abc","expiresAt":"2026-07-27T17:05:00.000Z"}"#;

    const CLAIM_CONFLICT: &str =
        r#"{"error":"Job already claimed","errorCode":"CLAIM_CONFLICT","status":"job_claimed"}"#;

    #[test]
    fn poll_pairing_pending_matches_deployed_backend_shape() {
        let parsed: PollPairingSessionResponse =
            parse_success_body(POLL_PENDING).expect("pending poll should parse");
        assert!(parsed.ok);
        assert_eq!(parsed.status, "pairing_pending");
        assert!(parsed.access_token.is_none());
    }

    #[test]
    fn poll_pairing_completed_with_tokens_matches_deployed_backend_shape() {
        let parsed: PollPairingSessionResponse =
            parse_success_body(POLL_COMPLETED).expect("completed poll should parse");
        assert!(parsed.ok);
        assert_eq!(parsed.status, "pairing_completed");
        assert_eq!(
            parsed.access_token.as_deref(),
            Some("access-test-token")
        );
        assert_eq!(parsed.refresh_token.as_deref(), Some("refresh-test-token"));
        assert_eq!(parsed.access_expires_in, Some(3600));
        assert_eq!(parsed.user_id.as_deref(), Some("user-1"));
    }

    #[test]
    fn poll_pairing_completed_without_tokens_matches_deployed_backend_shape() {
        let parsed: PollPairingSessionResponse =
            parse_success_body(POLL_COMPLETED_NO_TOKENS).expect("completed poll should parse");
        assert!(parsed.ok);
        assert_eq!(parsed.status, "pairing_completed");
        assert!(parsed.access_token.is_none());
        assert_eq!(parsed.device_id.as_deref(), Some("dev-abc"));
    }

    #[test]
    fn poll_pairing_error_body_parses_as_api_error() {
        let err = parse_error_body(403, POLL_ERROR);
        match err {
            ApiClientError::Http { status, message, code } => {
                assert_eq!(status, 403);
                assert_eq!(message, "Pairing session not found");
                assert_eq!(code.as_deref(), Some("PAIRING_NOT_FOUND"));
            }
            other => panic!("expected HTTP error, got {other:?}"),
        }
    }

    #[test]
    fn poll_pairing_error_body_deserializes_without_panic_when_missing_ok() {
        let parsed: PollPairingSessionResponse =
            parse_success_body(POLL_ERROR).expect("error JSON should deserialize without panic");
        assert!(!parsed.ok);
        assert_eq!(parsed.error.as_deref(), Some("Pairing session not found"));
        assert_eq!(parsed.error_code.as_deref(), Some("PAIRING_NOT_FOUND"));
    }

    #[test]
    fn unwraps_optional_data_wrapper() {
        let wrapped = r#"{"data":{"ok":true,"status":"pairing_pending","accessToken":null,"refreshToken":null,"deviceId":null,"userId":null,"dealershipId":null}}"#;
        let parsed: PollPairingSessionResponse =
            parse_success_body(wrapped).expect("wrapped poll should parse");
        assert!(parsed.ok);
        assert_eq!(parsed.status, "pairing_pending");
    }

    #[test]
    fn claim_job_success_matches_deployed_backend_shape() {
        let parsed: ClaimJobResponse =
            parse_success_body(CLAIM_SUCCESS).expect("claim response should parse");
        assert!(parsed.ok);
        assert_eq!(parsed.job_id, "job-123");
        assert_eq!(
            parsed.scoped_job_token.as_deref(),
            Some("scoped.jwt.token")
        );
    }

    #[test]
    fn claim_job_conflict_parses_as_http_error() {
        let err = parse_error_body(409, CLAIM_CONFLICT);
        match err {
            ApiClientError::Http { status, code, .. } => {
                assert_eq!(status, 409);
                assert_eq!(code.as_deref(), Some("CLAIM_CONFLICT"));
            }
            other => panic!("expected HTTP error, got {other:?}"),
        }
    }

    #[test]
    fn update_status_success_matches_deployed_backend_shape() {
        let parsed: serde_json::Value =
            parse_success_body(UPDATE_STATUS_SUCCESS).expect("update_status should parse");
        assert_eq!(parsed["ok"], true);
        assert_eq!(parsed["status"], "browser_opening");
    }

    #[test]
    fn complete_test_job_matches_deployed_backend_shape() {
        let parsed: serde_json::Value =
            parse_success_body(COMPLETE_TEST_JOB).expect("complete_job should parse");
        assert_eq!(parsed["ok"], true);
        assert_eq!(parsed["status"], "posted");
        assert_eq!(parsed["test"], true);
    }

    #[test]
    fn redeem_launch_session_success_matches_contract_shape() {
        let parsed: RedeemLaunchSessionResponse =
            parse_success_body(REDEEM_SUCCESS).expect("redeem should parse");
        assert!(parsed.ok);
        assert_eq!(parsed.nonce.as_deref(), Some("nonce-abc"));
    }
}
