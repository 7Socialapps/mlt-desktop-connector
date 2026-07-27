/**
 * Typed HTTP client for the browser-connector edge function.
 * Mirrors mlt/src/lib/webposter/desktopConnectorApi.ts desktop-side contract.
 */
import type {
  ApiErrorBody,
  AuthenticateDeviceRequest,
  AuthenticateDeviceResponse,
  ConnectorAuthHeaders,
  ConnectorClientConfig,
  CreatePairingCodeRequest,
  CreatePairingCodeResponse,
  ExchangePairingCodeRequest,
  ExchangePairingCodeResponse,
  HeartbeatRequest,
  HeartbeatResponse,
  PollJobsRequest,
  PollJobsResponse,
  RegisterDeviceRequest,
  RegisterDeviceResponse,
} from "./types";

const FUNCTION_NAME = "browser-connector";
const DEFAULT_TIMEOUT_MS = 15_000;

export class ConnectorApiError extends Error {
  readonly status: number;
  readonly errorCode?: string;

  constructor(message: string, status: number, errorCode?: string) {
    super(message);
    this.name = "ConnectorApiError";
    this.status = status;
    this.errorCode = errorCode;
  }
}

function buildHeaders(
  config: ConnectorClientConfig,
  auth: ConnectorAuthHeaders = {},
): Record<string, string> {
  const headers: Record<string, string> = {
    "Content-Type": "application/json",
    apikey: config.supabaseAnonKey,
    Authorization: `Bearer ${config.supabaseAnonKey}`,
  };

  if (auth.deviceAccessToken) {
    headers["x-connector-device-token"] = auth.deviceAccessToken;
  }
  if (auth.neonSessionToken) {
    headers["x-neon-session-token"] = auth.neonSessionToken;
  }

  return headers;
}

async function invoke<T>(
  config: ConnectorClientConfig,
  body: Record<string, unknown>,
  auth: ConnectorAuthHeaders = {},
  timeoutMs = DEFAULT_TIMEOUT_MS,
): Promise<T> {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), timeoutMs);

  try {
    const response = await fetch(
      `${config.supabaseUrl}/functions/v1/${FUNCTION_NAME}`,
      {
        method: "POST",
        headers: buildHeaders(config, auth),
        body: JSON.stringify(body),
        signal: controller.signal,
      },
    );

    const data = (await response.json().catch(() => null)) as (T & ApiErrorBody) | null;

    if (!response.ok || !data) {
      throw new ConnectorApiError(
        data?.error ?? `Edge function returned ${response.status}`,
        response.status,
        data?.errorCode,
      );
    }

    return data;
  } catch (err) {
    if (err instanceof ConnectorApiError) throw err;
    if (err instanceof DOMException && err.name === "AbortError") {
      throw new ConnectorApiError("Request timed out", 0, "TIMEOUT");
    }
    throw new ConnectorApiError(
      err instanceof Error ? err.message : "Request failed",
      0,
      "NETWORK_ERROR",
    );
  } finally {
    clearTimeout(timer);
  }
}

export function createConnectorApiClient(config: ConnectorClientConfig) {
  if (config.environment !== "staging") {
    throw new Error("Only staging environment is supported in Milestone A");
  }

  return {
    registerDevice(
      request: Omit<RegisterDeviceRequest, "action">,
      neonSessionToken: string,
    ) {
      return invoke<RegisterDeviceResponse>(
        config,
        { action: "register_device", ...request },
        { neonSessionToken },
      );
    },

    authenticateDevice(request: Omit<AuthenticateDeviceRequest, "action">) {
      return invoke<AuthenticateDeviceResponse>(config, {
        action: "authenticate_device",
        ...request,
      });
    },

    heartbeat(
      request: Omit<HeartbeatRequest, "action">,
      auth: ConnectorAuthHeaders,
    ) {
      return invoke<HeartbeatResponse>(
        config,
        { action: "heartbeat", ...request },
        auth,
      );
    },

    pollJobs(
      request: Omit<PollJobsRequest, "action">,
      auth: ConnectorAuthHeaders,
    ) {
      return invoke<PollJobsResponse>(
        config,
        { action: "poll_jobs", ...request },
        auth,
      );
    },

    /** Proposed — backend action not yet deployed. */
    createPairingCode(request: Omit<CreatePairingCodeRequest, "action">) {
      return invoke<CreatePairingCodeResponse>(config, {
        action: "create_pairing_code",
        ...request,
      });
    },

    /** Proposed — backend action not yet deployed; requires Neon session. */
    exchangePairingCode(
      request: Omit<ExchangePairingCodeRequest, "action">,
      neonSessionToken: string,
    ) {
      return invoke<ExchangePairingCodeResponse>(
        config,
        { action: "exchange_pairing_code", ...request },
        { neonSessionToken },
      );
    },
  };
}

export type ConnectorApiClient = ReturnType<typeof createConnectorApiClient>;

export const DESKTOP_CONNECTOR_CONTRACT_VERSION = 1;
