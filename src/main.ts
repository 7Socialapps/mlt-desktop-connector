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

const app = document.querySelector<HTMLDivElement>("#app")!;
let pairingActionError: string | null = null;
let browserActionError: string | null = null;
let actionInProgress: string | null = null;
let diagnosticsReport: ConnectionTestReport | null = null;

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
    return "Use Open Facebook Login and sign in manually.";
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
) {
  const displayError = pairing.error ?? pairingActionError;
  const remediation = remediationForBrowser(browser);
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
  const canRestart =
    browser.sidecar_running && browser.chromium_installed && !browserBusy;
  const canResetProfile = !browserReady && !browserBusy;
  const canReconnect = status.needs_reconnect || !status.paired;

  app.innerHTML = `
    <main class="status-window">
      <header>
        <h1>MLT Desktop Connector</h1>
        <span class="badge badge-${status.connection_state}">${labelize(status.connection_state)}</span>
      </header>
      <dl>
        <div><dt>Version</dt><dd>${status.connector_version}</dd></div>
        <div><dt>Environment</dt><dd>${status.environment}</dd></div>
        <div><dt>Device ID</dt><dd class="mono">${status.device_id}</dd></div>
        <div><dt>Paired</dt><dd>${status.paired ? "Yes" : "No"}</dd></div>
        <div><dt>Current job</dt><dd>${status.current_job_id ?? "—"}</dd></div>
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
        <h2>Browser</h2>
        ${actionInProgress ? `<p class="helper progress" role="status">Working: ${labelize(actionInProgress)}…</p>` : ""}
        <dl>
          <div><dt>Browser</dt><dd><span class="badge badge-browser-${browser.status}">${labelize(browser.status)}</span></dd></div>
          <div><dt>Chromium</dt><dd>${browser.chromium_installed ? (browser.playwright_version ?? "available") : "not installed"}</dd></div>
          <div><dt>Profile</dt><dd>${labelize(browser.profile_status)}</dd></div>
          <div><dt>Facebook</dt><dd>${labelize(browser.facebook_session.state)}</dd></div>
          <div><dt>Marketplace</dt><dd>${labelize(browser.marketplace.status)}</dd></div>
          <div><dt>Last browser error</dt><dd>${browser.last_error_code ?? browser.last_error ?? "—"}</dd></div>
          <div><dt>Sidecar</dt><dd>${browser.sidecar_running ? "running" : "stopped"}</dd></div>
          <div><dt>Last health check</dt><dd>${browser.last_health_check_at ?? "—"}</dd></div>
        </dl>
        ${
          !browser.chromium_installed && browser.enabled
            ? `<p class="helper">Run <code>npm run browser:install</code> once to download Chromium. The connector will not download browsers automatically.</p>`
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
          <button id="open-facebook-login" ${canFacebookLogin ? "" : "disabled"}>Open Facebook Login</button>
          <button id="open-marketplace" ${canMarketplace ? "" : "disabled"}>Open Marketplace</button>
          <button id="restart-browser" ${canRestart ? "" : "disabled"}>Restart Browser</button>
          <button id="run-diagnostics" ${actionInProgress ? "disabled" : ""}>Run Diagnostics</button>
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
  document.querySelector("#open-facebook-login")?.addEventListener("click", () => void openFacebookLogin());
  document.querySelector("#open-marketplace")?.addEventListener("click", () => void openMarketplace());
  document.querySelector("#restart-browser")?.addEventListener("click", () => void restartBrowser());
  document.querySelector("#run-diagnostics")?.addEventListener("click", () => void runDiagnostics());
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

async function restartBrowser() {
  await withAction("restart_browser", async () => {
    try {
      await invoke<BrowserManagerSnapshot>("browser_restart");
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
  const [status, browser] = await Promise.all([
    invoke<ConnectorStatus>("get_status"),
    invoke<BrowserManagerSnapshot>("get_browser_status"),
  ]);
  render(status, pairing, browser);
}

async function refresh() {
  try {
    const [status, pairing, browser] = await Promise.all([
      invoke<ConnectorStatus>("get_status"),
      invoke<PairingState>("get_pairing_state"),
      invoke<BrowserManagerSnapshot>("get_browser_status"),
    ]);
    render(status, pairing, browser);
  } catch (err) {
    app.innerHTML = `<p class="error">Failed to load status: ${String(err)}</p>`;
  }
}

async function main() {
  await refresh();
  await listen("connector://status-changed", () => void refresh());
  await listen("connector://pairing-changed", () => void refresh());
  await listen("connector://browser-changed", () => void refresh());
  setInterval(() => void refresh(), 5_000);
}

void main();
