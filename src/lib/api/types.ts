/** MLT Desktop Connector semantic version — keep in sync with src-tauri/src/version.rs */
export const CONNECTOR_VERSION = "1.0.0";

export const CONNECTOR_CONTRACT_VERSION = 1;

export const DEFAULT_CAPABILITIES = [
  "facebook_marketplace_posting",
  "screenshots",
] as const;

export type ConnectorOs = "windows" | "macos" | "linux" | "unknown";

export type DeviceStatus =
  | "connector_ready"
  | "connector_offline"
  | "update_required";

export type FacebookSessionState =
  | "unknown"
  | "signed_in"
  | "signed_out"
  | "expired";

export type ConnectorJobStatus =
  | "job_available"
  | "job_claimed"
  | "browser_opening"
  | "checking_facebook_session"
  | "facebook_login_required"
  | "marketplace_opening"
  | "images_downloading"
  | "images_uploading"
  | "fields_filling"
  | "ready_for_review"
  | "publishing"
  | "verifying"
  | "posted"
  | "needs_attention"
  | "failed"
  | "canceled";

export interface DesktopDevice {
  id: string;
  device_name: string;
  os: ConnectorOs;
  connector_version: string | null;
  status: DeviceStatus;
  last_heartbeat_at: string | null;
  connected_at: string | null;
  user_id: string | null;
  dealership_id: string | null;
  facebook_session_state?: FacebookSessionState;
  current_job_id?: string | null;
  capabilities?: string[];
}

export interface ConnectorJobSummary {
  id: string;
  inventory_item_id: string;
  status: ConnectorJobStatus;
  progress_percentage: number;
  current_step: string;
  created_at: string;
  expires_at: string;
}

export interface RegisterDeviceRequest {
  action: "register_device";
  deviceId: string;
  connectorVersion: string;
  userId: string;
  dealershipId: string;
  deviceName?: string;
  browserName?: string;
}

export interface RegisterDeviceResponse {
  ok: boolean;
  deviceId: string;
  status: DeviceStatus;
  connectorVersion: string;
  contractVersion: number;
  accessToken: string | null;
  refreshToken: string | null;
  accessExpiresIn: number | null;
  refreshExpiresIn: number | null;
  error?: string;
  errorCode?: string;
}

export interface AuthenticateDeviceRequest {
  action: "authenticate_device";
  refreshToken: string;
}

export interface AuthenticateDeviceResponse {
  ok: boolean;
  accessToken: string;
  refreshToken: string;
  accessExpiresIn: number;
  refreshExpiresIn: number;
  error?: string;
  errorCode?: string;
}

export interface HeartbeatRequest {
  action: "heartbeat";
  deviceId: string;
  userId: string;
  dealershipId: string;
  connectorVersion: string;
  os?: ConnectorOs;
  capabilities?: string[];
  facebook_session_state?: FacebookSessionState;
  currentJobId?: string | null;
  lastError?: string;
}

export interface HeartbeatResponse {
  ok: boolean;
  status: DeviceStatus;
  lastHeartbeatAt: string;
  error?: string;
  errorCode?: string;
}

export interface PollJobsRequest {
  action: "poll_jobs";
  deviceId: string;
  userId: string;
  dealershipId: string;
  connectorVersion: string;
}

export interface PollJobsResponse {
  ok: boolean;
  connectorStatus: DeviceStatus;
  jobsAvailable: number;
  jobs: ConnectorJobSummary[];
  error?: string;
  errorCode?: string;
}

/** Proposed pairing flow — not yet implemented on backend (Milestone B). */
export interface CreatePairingCodeRequest {
  action: "create_pairing_code";
  deviceId: string;
  connectorVersion: string;
  os?: ConnectorOs;
  capabilities?: string[];
}

export interface CreatePairingCodeResponse {
  ok: boolean;
  pairingCode: string;
  expiresAt: string;
  error?: string;
  errorCode?: string;
}

export interface ExchangePairingCodeRequest {
  action: "exchange_pairing_code";
  pairingCode: string;
  userId: string;
  dealershipId: string;
}

export interface ExchangePairingCodeResponse {
  ok: boolean;
  deviceId: string;
  accessToken: string;
  refreshToken: string;
  accessExpiresIn: number;
  refreshExpiresIn: number;
  error?: string;
  errorCode?: string;
}

export type BrowserConnectorAction =
  | RegisterDeviceRequest["action"]
  | AuthenticateDeviceRequest["action"]
  | HeartbeatRequest["action"]
  | PollJobsRequest["action"]
  | "update_device_info"
  | "claim_job"
  | "get_payload"
  | "update_status"
  | "upload_evidence"
  | "report_login_required"
  | "report_attention_required"
  | "complete_job"
  | "fail_job"
  | "release_job"
  | CreatePairingCodeRequest["action"]
  | ExchangePairingCodeRequest["action"];

export interface ApiErrorBody {
  error?: string;
  errorCode?: string;
  status?: DeviceStatus;
}

export interface ConnectorClientConfig {
  supabaseUrl: string;
  supabaseAnonKey: string;
  environment: "staging";
}

export interface ConnectorAuthHeaders {
  deviceAccessToken?: string;
  neonSessionToken?: string;
}
