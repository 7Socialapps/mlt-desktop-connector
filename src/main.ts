import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import "./styles.css";

interface ConnectorStatus {
  device_id: string;
  connector_version: string;
  environment: string;
  paired: boolean;
  needs_reconnect: boolean;
  connection_state: string;
  last_heartbeat_at: string | null;
  last_error: string | null;
  current_job_id: string | null;
  deep_link_message: string | null;
  launch_session_id: string | null;
  launch_status: string | null;
}

interface ChromiumProvisionState {
  active: boolean;
  progress: number;
  message: string;
  error: string | null;
}

interface PairingState {
  active: boolean;
  pairing_code: string | null;
  expires_at: string | null;
  status: string;
  error: string | null;
}

interface FacebookSessionSnapshot {
  state: string;
  checked_at: string | null;
  current_url: string | null;
  marketplace_accessible: boolean;
  reason_code: string | null;
}

interface MarketplaceSnapshot {
  status: string;
  checked_at: string | null;
  current_url: string | null;
  reason_code: string | null;
  screenshot_path: string | null;
}

interface BrowserManagerSnapshot {
  status: string;
  enabled: boolean;
  playwright_installed: boolean;
  playwright_version: string | null;
  chromium_installed: boolean;
  chromium_path: string | null;
  node_version: string | null;
  last_error: string | null;
  last_error_code: string | null;
  checked_at: string | null;
  sidecar_running: boolean;
  browser_pid: number | null;
  active_page_url: string | null;
  active_page_title: string | null;
  restart_attempts: number;
  max_restart_attempts: number;
  last_health_check_at: string | null;
  auto_restart_enabled: boolean;
  profile_status: string;
  profile_path: string | null;
  facebook_session: FacebookSessionSnapshot;
  marketplace: MarketplaceSnapshot;
}

interface RuntimeStatus {
  browser_state: string;
  facebook_session_state: string;
  marketplace_state: string;
  current_destination: string | null;
  current_service: string | null;
  last_navigation_error: string | null;
  current_url: string | null;
  navigation_target: string | null;
  last_successful_url: string | null;
  navigation_started_at: string | null;
  navigation_completed_at: string | null;
  navigation_failure_reason: string | null;
  timeout_reason: string | null;
}

interface ConnectionCheck {
  id: string;
  status: string;
  label: string;
  detail: string;
  error_code: string | null;
  checked_at: string;
}

interface ConnectionTestReport {
  checks: ConnectionCheck[];
  overall_status: string;
  checked_at: string;
}

interface JobProgressSnapshot {
  job_id: string;
  phase: string;
  progress: number;
  current_step: string;
}

const VALIDATION_CHECKPOINTS = [
  "Launch Browser — Chromium opens with persistent profile",
  "Check Facebook Session — confirms logged-in state",
  "Open Marketplace — navigates to marketplace home",
  "Open Vehicle Create Form — lands on /marketplace/create/vehicle",
  "Submit prepare_for_review job — connector fills form and uploads images",
  "Confirm ready_for_review — form editable, Next/Publish visible, NOT clicked",
  "Publish manually in Chromium — inventory must NOT auto-mark posted",
];

const app = document.querySelector<HTMLDivElement>("#app")!;
let pairingActionError: string | null = null;
let browserActionError: string | null = null;
let actionInProgress: string | null = null;
let diagnosticsReport: ConnectionTestReport | null = null;
let jobProgress: JobProgressSnapshot | null = null;
let showValidationPanel = false;
let chromiumProvision: ChromiumProvisionState = {
  active: false,
  progress: 0,
  message: "Checking browser components…",
  error: null,
};

type WelcomeStep =
  | "checking"
  | "download_browser"
  | "pair"
  | "connect_facebook"
  | "marketplace"
  | "ready";

function computeWelcomeStep(
  status: ConnectorStatus,
  browser: BrowserManagerSnapshot,
  runtime: RuntimeStatus,
): { step: WelcomeStep; title: string; detail: string } {
  if (status.connection_state === "starting") {
    return {
      step: "checking",
      title: "Initializing Desktop Connector",
      detail: "Starting background services. This usually takes a few seconds.",
    };
  }
  if (chromiumProvision.active || (!browser.chromium_installed && browser.enabled)) {
    return {
      step: "download_browser",
      title: "Setting up your browser",
      detail: chromiumProvision.message,
    };
  }
  if (status.needs_reconnect) {
    return {
      step: "pair",
      title: "This device needs to be paired again",
      detail: status.last_error ?? "Your dashboard access was revoked. Pair again to continue.",
    };
  }
  if (!status.paired) {
    return {
      step: "pair",
      title: "Pair with your MLT Dashboard",
      detail: "Start pairing below, then enter the code in Web Poster → Pair Desktop Connector.",
    };
  }
  const loggedIn =
    runtime.facebook_session_state === "logged_in" ||
    browser.facebook_session.state === "facebook_logged_in";
  if (!loggedIn) {
    return {
      step: "connect_facebook",
      title: "Connect Facebook",
      detail:
        status.deep_link_message ??
        "Sign into Facebook in the browser window. Your password stays on Facebook — never in MLT.",
    };
  }
  if (runtime.marketplace_state !== "ready" && browser.marketplace.status !== "marketplace_ready") {
    return {
      step: "marketplace",
      title: "Open Facebook Marketplace",
      detail: "Confirm Marketplace loads in the browser before posting from the dashboard.",
    };
  }
  return {
    step: "ready",
    title: "You're ready to post",
    detail: "Keep this app running in the tray while you work in the MLT Dashboard.",
  };
}

function labelize(value: string): string {
  return value.replaceAll("_", " ");
}

function remediationForBrowser(browser: BrowserManagerSnapshot): string | null {
  if (browser.profile_status === "profile_locked") {
    return "Close other connector instances, then restart.";
  }
  if (
    browser.profile_status === "profile_corrupt" ||
    browser.profile_status === "profile_reset_required"
  ) {
    return "Reset Browser Profile after stopping the browser.";
  }
  const fb = browser.facebook_session.state;
  if (fb === "facebook_logged_out" || fb === "facebook_session_expired") {
    return "Log into Facebook in the browser window, then click Check Facebook Session.";
  }
  if (fb === "facebook_checkpoint" || fb === "facebook_mfa_required") {
    return "Complete Facebook security steps manually in the browser.";
  }
  if (browser.marketplace.status === "marketplace_login_required") {
    return "Sign into Facebook before opening Marketplace.";
  }
  if (browser.last_error_code === "BROWSER_TERMINAL_ERROR") {
    return "Restart Browser or relaunch the connector.";
  }
  return null;
}

function render(
  status: ConnectorStatus,
  pairing: PairingState,
  browser: BrowserManagerSnapshot,
  runtime: RuntimeStatus,
) {
  const displayError = pairing.error ?? pairingActionError;
  const remediation = remediationForBrowser(browser);
  const loggedOut =
    browser.facebook_session.state === "facebook_logged_out" ||
    browser.facebook_session.state === "facebook_session_expired" ||
    runtime.facebook_session_state === "logged_out";
  const readyForReview = jobProgress?.phase === "ready_for_review";
  const browserReady = browser.status === "browser_ready";
  const browserBusy = ["browser_starting", "browser_restarting"].includes(browser.status);
  const canLaunch =
    browser.enabled &&
    browser.chromium_installed &&
    browser.sidecar_running &&
    !browserBusy &&
    !actionInProgress;
  const canFacebookLogin = browserReady && !actionInProgress;
  const canMarketplace = browserReady && !actionInProgress;
  const canVehicleCreate = browserReady && !actionInProgress;
  const canRestart =
    browser.sidecar_running && browser.chromium_installed && !browserBusy;
  const canResetProfile = !browserReady && !browserBusy;
  const canReconnect = status.needs_reconnect || !status.paired;
  const canCancelOperation = Boolean(actionInProgress);

  const welcome = computeWelcomeStep(status, browser, runtime);
  const dashboardMessage = status.deep_link_message;

  app.innerHTML = `
    <main class="status-window">
      <header>
        <h1>MLT Desktop Connector</h1>
        <span class="badge badge-${status.connection_state}">${labelize(status.connection_state)}</span>
      </header>

      <section class="welcome-section">
        <h2>Getting started</h2>
        <p class="welcome-title">${welcome.title}</p>
        <p class="helper">${welcome.detail}</p>
        ${
          chromiumProvision.active
            ? `<div class="progress-bar" role="progressbar" aria-valuenow="${chromiumProvision.progress}" aria-valuemin="0" aria-valuemax="100">
                <div class="progress-fill" style="width:${chromiumProvision.progress}%"></div>
              </div>`
            : ""
        }
        ${
          chromiumProvision.error
            ? `<p class="error" role="alert">${chromiumProvision.error}</p>`
            : ""
        }
        ${
          dashboardMessage
            ? `<p class="helper dashboard-request" role="status">${dashboardMessage}</p>`
            : ""
        }
      </section>

      <dl>
        <div><dt>Version</dt><dd>${status.connector_version}</dd></div>
        <div><dt>Environment</dt><dd>${status.environment}</dd></div>
        <div><dt>Device ID</dt><dd class="mono">${status.device_id}</dd></div>
        <div><dt>Paired</dt><dd>${status.paired ? "Yes" : "No"}</dd></div>
        <div><dt>Current job</dt><dd>${status.current_job_id ?? "—"}</dd></div>
        ${
          jobProgress
            ? `<div><dt>Job phase</dt><dd>${labelize(jobProgress.phase)} (${jobProgress.progress}%)</dd></div>
               <div><dt>Job step</dt><dd>${jobProgress.current_step}</dd></div>`
            : ""
        }
        ${
          readyForReview
            ? `<div class="helper remediation" role="note">Listing prepared. Review the Facebook form and publish manually.</div>`
            : ""
        }
        ${
          loggedOut
            ? `<div class="helper" role="note">Log into Facebook in the browser window, then click Check Facebook Session.</div>`
            : ""
        }
        ${
          status.needs_reconnect
            ? `<div class="error"><dt>Reconnect required</dt><dd>${status.last_error ?? "Reconnect device — start pairing again."}</dd></div>`
            : ""
        }
        <div><dt>Last heartbeat</dt><dd>${status.last_heartbeat_at ?? "—"}</dd></div>
        ${
          status.last_error && !status.needs_reconnect
            ? `<div class="error"><dt>Last error</dt><dd>${status.last_error}</dd></div>`
            : ""
        }
      </dl>

      <section class="browser-section">
        <h2>Browser & Runtime</h2>
        ${actionInProgress ? `<p class="helper progress" role="status">Working: ${labelize(actionInProgress)}…</p>` : ""}
        <dl>
          <div><dt>Browser</dt><dd><span class="badge badge-browser-${browser.status}">${labelize(runtime.browser_state)}</span></dd></div>
          <div><dt>Facebook Session</dt><dd>${labelize(runtime.facebook_session_state)}</dd></div>
          <div><dt>Marketplace</dt><dd>${labelize(runtime.marketplace_state)}</dd></div>
          <div><dt>Current Destination</dt><dd>${runtime.current_destination ? labelize(runtime.current_destination) : "—"}</dd></div>
          <div><dt>Current URL</dt><dd class="mono">${runtime.current_url ?? browser.active_page_url ?? "—"}</dd></div>
          <div><dt>Navigation Target</dt><dd class="mono">${runtime.navigation_target ?? "—"}</dd></div>
          <div><dt>Last Successful URL</dt><dd class="mono">${runtime.last_successful_url ?? "—"}</dd></div>
          <div><dt>Navigation Started</dt><dd>${runtime.navigation_started_at ?? "—"}</dd></div>
          <div><dt>Navigation Completed</dt><dd>${runtime.navigation_completed_at ?? "—"}</dd></div>
          <div><dt>Navigation Failure</dt><dd>${runtime.navigation_failure_reason ?? "—"}</dd></div>
          <div><dt>Timeout Reason</dt><dd>${runtime.timeout_reason ?? "—"}</dd></div>
          <div><dt>Current Service</dt><dd>${runtime.current_service ? labelize(runtime.current_service) : "—"}</dd></div>
          <div><dt>Last Navigation Error</dt><dd>${runtime.last_navigation_error ?? "—"}</dd></div>
          <div><dt>Chromium</dt><dd>${browser.chromium_installed ? (browser.playwright_version ?? "available") : "not installed"}</dd></div>
          <div><dt>Profile</dt><dd>${labelize(browser.profile_status)}</dd></div>
          <div><dt>Last browser error</dt><dd>${browser.last_error_code ?? browser.last_error ?? "—"}</dd></div>
          <div><dt>Sidecar</dt><dd>${browser.sidecar_running ? "running" : "stopped"}</dd></div>
          <div><dt>Last health check</dt><dd>${browser.last_health_check_at ?? "—"}</dd></div>
        </dl>
        ${
          !browser.chromium_installed && browser.enabled && !chromiumProvision.active
            ? `<p class="helper">The connector will download Chromium automatically on first run.</p>`
            : ""
        }
        ${
          remediation
            ? `<p class="helper remediation" role="note">${remediation}</p>`
            : ""
        }
        ${
          browserActionError || browser.last_error
            ? `<p class="error" role="alert">${browserActionError ?? browser.last_error}</p>`
            : ""
        }
        <div class="button-row">
          <button id="launch-browser" ${canLaunch ? "" : "disabled"}>Launch Browser</button>
          <button id="check-facebook-session" ${canFacebookLogin ? "" : "disabled"}>Check Facebook Session</button>
          <button id="open-facebook-login" ${canFacebookLogin ? "" : "disabled"}>Open Facebook Login</button>
          <button id="open-marketplace" ${canMarketplace ? "" : "disabled"}>Open Marketplace</button>
          <button id="open-vehicle-create" ${canVehicleCreate ? "" : "disabled"}>Open Vehicle Create Form</button>
          <button id="cancel-operation" ${canCancelOperation ? "" : "disabled"}>Cancel Current Operation</button>
          <button id="restart-browser" ${canRestart ? "" : "disabled"}>Restart Browser</button>
          <button id="run-runtime-diagnostics" ${actionInProgress ? "disabled" : ""}>Run Runtime Diagnostics</button>
          <button id="run-diagnostics" ${actionInProgress ? "disabled" : ""}>Run Diagnostics</button>
          <button id="toggle-validation" ${actionInProgress ? "disabled" : ""}>Real-Device Validation</button>
          <button id="open-log-folder">Open Log Folder</button>
          <button id="reset-profile" ${canResetProfile ? "" : "disabled"}>Reset Browser Profile</button>
          <button id="reconnect-device" ${canReconnect ? "" : "disabled"}>Reconnect Device</button>
        </div>
        ${
          diagnosticsReport
            ? `<div class="diagnostics">
                <h3>Diagnostics (${diagnosticsReport.overall_status})</h3>
                <ul>${diagnosticsReport.checks
                  .map(
                    (c) =>
                      `<li class="check-${c.status}"><strong>${c.label}</strong>: ${c.detail}${c.error_code ? ` (${c.error_code})` : ""}</li>`,
                  )
                  .join("")}</ul>
              </div>`
            : ""
        }
        ${
          showValidationPanel
            ? `<div class="diagnostics validation-panel">
                <h3>Real-device validation checkpoints</h3>
                <ol>${VALIDATION_CHECKPOINTS.map((c) => `<li>${c}</li>`).join("")}</ol>
              </div>`
            : ""
        }
      </section>

      <section class="pairing-section">
        <h2>Pair with MLT Dashboard</h2>
        <p class="helper">
          Start pairing here, then sign into the MLT dashboard and enter this code under
          Web Poster → Pair Desktop Connector.
        </p>
        ${
          pairing.pairing_code
            ? `<div class="pairing-code">${pairing.pairing_code}</div>`
            : `<p class="helper">No active pairing session.</p>`
        }
        <p class="helper">Status: ${pairing.status}${displayError ? ` — ${displayError}` : ""}</p>
        ${
          displayError
            ? `<p class="error" role="alert">Pairing error: ${displayError}</p>`
            : ""
        }
        <button id="start-pairing" ${pairing.active ? "disabled" : ""}>
          ${pairing.active ? "Waiting for dashboard approval…" : "Start pairing session"}
        </button>
      </section>

      <footer>
        <p>Runs in the system tray. Close this window to hide — the connector keeps running.</p>
      </footer>
    </main>
  `;

  bindActions(status, pairing, browser);
}

function bindActions(
  _status: ConnectorStatus,
  _pairing: PairingState,
  _browser: BrowserManagerSnapshot,
) {
  document.querySelector("#start-pairing")?.addEventListener("click", () => void startPairing());
  document.querySelector("#launch-browser")?.addEventListener("click", () => void launchBrowser());
  document.querySelector("#check-facebook-session")?.addEventListener("click", () => void checkFacebookSession());
  document.querySelector("#open-facebook-login")?.addEventListener("click", () => void openFacebookLogin());
  document.querySelector("#open-marketplace")?.addEventListener("click", () => void openMarketplace());
  document.querySelector("#open-vehicle-create")?.addEventListener("click", () => void openVehicleCreate());
  document.querySelector("#cancel-operation")?.addEventListener("click", () => void cancelOperation());
  document.querySelector("#restart-browser")?.addEventListener("click", () => void restartBrowser());
  document.querySelector("#run-runtime-diagnostics")?.addEventListener("click", () => void runRuntimeDiagnostics());
  document.querySelector("#run-diagnostics")?.addEventListener("click", () => void runDiagnostics());
  document.querySelector("#toggle-validation")?.addEventListener("click", () => {
    showValidationPanel = !showValidationPanel;
    void refresh();
  });
  document.querySelector("#open-log-folder")?.addEventListener("click", () => void openLogFolder());
  document.querySelector("#reset-profile")?.addEventListener("click", () => void resetProfile());
  document.querySelector("#reconnect-device")?.addEventListener("click", () => void reconnectDevice());
}

async function withAction(name: string, fn: () => Promise<void>) {
  actionInProgress = name;
  browserActionError = null;
  await refresh();
  try {
    await fn();
  } finally {
    actionInProgress = null;
    await refresh();
  }
}

async function startPairing() {
  pairingActionError = null;
  try {
    const result = await invoke<PairingState>("start_pairing_session", { deviceName: null });
    pairingActionError = null;
    await refreshWith(result);
  } catch (err) {
    pairingActionError = err instanceof Error ? err.message : String(err);
    await refresh();
  }
}

async function launchBrowser() {
  await withAction("launch_browser", async () => {
    try {
      await invoke<BrowserManagerSnapshot>("browser_launch");
    } catch (err) {
      browserActionError = err instanceof Error ? err.message : String(err);
    }
  });
}

async function checkFacebookSession() {
  await withAction("check_facebook_session", async () => {
    try {
      await invoke<BrowserManagerSnapshot>("browser_detect_facebook_session");
    } catch (err) {
      browserActionError = err instanceof Error ? err.message : String(err);
    }
  });
}

async function openFacebookLogin() {
  await withAction("open_facebook_login", async () => {
    try {
      await invoke<BrowserManagerSnapshot>("browser_open_facebook_login");
    } catch (err) {
      browserActionError = err instanceof Error ? err.message : String(err);
    }
  });
}

async function openMarketplace() {
  await withAction("open_marketplace", async () => {
    try {
      await invoke<BrowserManagerSnapshot>("browser_open_marketplace", { createVehicle: false });
    } catch (err) {
      browserActionError = err instanceof Error ? err.message : String(err);
    }
  });
}

async function openVehicleCreate() {
  await withAction("open_vehicle_create", async () => {
    try {
      await invoke<BrowserManagerSnapshot>("browser_open_vehicle_create");
    } catch (err) {
      browserActionError = err instanceof Error ? err.message : String(err);
    }
  });
}

async function cancelOperation() {
  await withAction("cancel_operation", async () => {
    try {
      await invoke("runtime_cancel_operation");
    } catch (err) {
      browserActionError = err instanceof Error ? err.message : String(err);
    }
  });
}

async function restartBrowser() {
  await withAction("restart_browser", async () => {
    try {
      await invoke<BrowserManagerSnapshot>("browser_restart");
    } catch (err) {
      browserActionError = err instanceof Error ? err.message : String(err);
    }
  });
}

async function runRuntimeDiagnostics() {
  await withAction("runtime_diagnostics", async () => {
    try {
      await invoke("runtime_diagnostics_snapshot");
    } catch (err) {
      browserActionError = err instanceof Error ? err.message : String(err);
    }
  });
}

async function runDiagnostics() {
  await withAction("run_diagnostics", async () => {
    try {
      diagnosticsReport = await invoke<ConnectionTestReport>("run_connection_tests_cmd");
    } catch (err) {
      browserActionError = err instanceof Error ? err.message : String(err);
    }
  });
}

async function openLogFolder() {
  try {
    await invoke("open_log_folder");
  } catch (err) {
    browserActionError = err instanceof Error ? err.message : String(err);
    await refresh();
  }
}

async function resetProfile() {
  const confirmed = window.confirm(
    "Reset the browser profile? You will need to sign into Facebook again. The browser must be stopped first.",
  );
  if (!confirmed) return;
  await withAction("reset_profile", async () => {
    try {
      await invoke<BrowserManagerSnapshot>("browser_reset_profile");
    } catch (err) {
      browserActionError = err instanceof Error ? err.message : String(err);
    }
  });
}

async function reconnectDevice() {
  await withAction("reconnect_device", async () => {
    try {
      await invoke<ConnectorStatus>("reconnect_device");
    } catch (err) {
      browserActionError = err instanceof Error ? err.message : String(err);
    }
  });
}

async function refreshWith(pairing: PairingState) {
  const [status, browser, runtime, progress] = await Promise.all([
    invoke<ConnectorStatus>("get_status"),
    invoke<BrowserManagerSnapshot>("get_browser_status"),
    invoke<RuntimeStatus>("runtime_status"),
    invoke<JobProgressSnapshot | null>("get_job_progress"),
  ]);
  jobProgress = progress;
  render(status, pairing, browser, runtime);
}

async function refresh() {
  try {
    const [status, pairing, browser, runtime, progress, provision] = await Promise.all([
      invoke<ConnectorStatus>("get_status"),
      invoke<PairingState>("get_pairing_state"),
      invoke<BrowserManagerSnapshot>("get_browser_status"),
      invoke<RuntimeStatus>("runtime_status"),
      invoke<JobProgressSnapshot | null>("get_job_progress"),
      invoke<ChromiumProvisionState>("get_chromium_provision_state"),
    ]);
    jobProgress = progress;
    chromiumProvision = provision;
    render(status, pairing, browser, runtime);
  } catch (err) {
    app.innerHTML = `<p class="error">Failed to load status: ${String(err)}</p>`;
  }
}

async function main() {
  const refreshSafe = () => {
    void refresh().catch((err) => {
      app.innerHTML = `<section class="card"><h1>MLT Desktop Connector</h1><p class="error">Failed to load status: ${String(err)}</p><p class="helper">The app is still running. Try again in a moment or use Open Logs Folder after the UI loads.</p></section>`;
    });
  };

  void Promise.all([
    listen("connector://status-changed", refreshSafe),
    listen("connector://pairing-changed", refreshSafe),
    listen("connector://browser-changed", refreshSafe),
    listen("connector://deep-link-changed", refreshSafe),
    listen("connector://startup-ready", refreshSafe),
    listen<ChromiumProvisionState>("connector://chromium-provision", (event) => {
      chromiumProvision = event.payload;
      refreshSafe();
    }),
  ]);

  refreshSafe();
  setInterval(refreshSafe, 5_000);
}

void main();
