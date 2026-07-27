use std::time::Duration;

use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use thiserror::Error;

use crate::config::AppConfig;

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
}

pub struct ConnectorApiClient {
    http: reqwest::Client,
    config: AppConfig,
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

        Ok(headers)
    }

    async fn post<T: serde::de::DeserializeOwned>(
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
        let parsed: T = serde_json::from_str(&text).unwrap_or_else(|_| {
            serde_json::from_str("{}").expect("empty json fallback")
        });

        if !status.is_success() {
            if let Ok(err) = serde_json::from_str::<ApiErrorBody>(&text) {
                return Err(ApiClientError::Http {
                    status: status.as_u16(),
                    message: err.error.unwrap_or_else(|| text.clone()),
                    code: err.error_code,
                });
            }
            return Err(ApiClientError::Http {
                status: status.as_u16(),
                message: text,
                code: None,
            });
        }

        Ok(parsed)
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
                neon_session_token: None,
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
                neon_session_token: None,
            },
        )
        .await
    }

    /// Proposed pairing action — backend not yet deployed.
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
        }
    }
}
