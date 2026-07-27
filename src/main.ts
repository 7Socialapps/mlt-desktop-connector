import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/event";
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
}

interface PairingState {
  active: boolean;
  pairing_code: string | null;
  expires_at: string | null;
  status: string;
  error: string | null;
}

interface BrowserRuntimeSnapshot {
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
}

const app = document.querySelector<HTMLDivElement>("#app")!;
let pairingActionError: string | null = null;
let browserActionError: string | null = null;

function browserStatusLabel(status: string): string {
  return status.replaceAll("_", " ");
}

function render(
  status: ConnectorStatus,
  pairing: PairingState,
  browser: BrowserRuntimeSnapshot,
) {
  const displayError = pairing.error ?? pairingActionError;
  const canLaunchBrowser =
    browser.enabled &&
    browser.chromium_installed &&
    browser.status !== "browser_starting";

  app.innerHTML = `
    <main class="status-window">
      <header>
        <h1>MLT Desktop Connector</h1>
        <span class="badge badge-${status.connection_state}">${status.connection_state}</span>
      </header>
      <dl>
        <div><dt>Version</dt><dd>${status.connector_version}</dd></div>
        <div><dt>Environment</dt><dd>${status.environment}</dd></div>
        <div><dt>Device ID</dt><dd class="mono">${status.device_id}</dd></div>
        <div><dt>Paired</dt><dd>${status.paired ? "Yes" : "No"}</dd></div>
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
        <h2>Browser Runtime (Milestone 2.1)</h2>
        <dl>
          <div><dt>Runtime state</dt><dd><span class="badge badge-browser-${browser.status}">${browserStatusLabel(browser.status)}</span></dd></div>
          <div><dt>Playwright</dt><dd>${browser.playwright_installed ? (browser.playwright_version ?? "installed") : "not installed"}</dd></div>
          <div><dt>Chromium</dt><dd>${browser.chromium_installed ? "available" : "not installed"}</dd></div>
          <div><dt>Node</dt><dd>${browser.node_version ?? "—"}</dd></div>
          <div><dt>Last checked</dt><dd>${browser.checked_at ?? "—"}</dd></div>
        </dl>
        ${
          !browser.chromium_installed && browser.enabled
            ? `<p class="helper">Run <code>npm run browser:install</code> once to download Chromium (~150MB). The connector will not download browsers automatically.</p>`
            : ""
        }
        ${
          browser.last_error || browserActionError
            ? `<p class="error" role="alert">Browser: ${browserActionError ?? browser.last_error}</p>`
            : ""
        }
        <div class="button-row">
          <button id="detect-browser">Detect Runtime</button>
          <button id="launch-browser-test" ${canLaunchBrowser ? "" : "disabled"}>Launch Test Browser</button>
          <button id="close-browser-test" ${browser.status === "browser_ready" ? "" : "disabled"}>Close Test Browser</button>
        </div>
      </section>

      <section class="pairing-section">
        <h2>Pair with MLT Dashboard</h2>
        <p class="helper">
          Start pairing here, then sign into the MLT dashboard and enter this code under
          Web Poster → Pair Desktop Connector. No MLT password is collected in this app.
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

  document.querySelector("#start-pairing")?.addEventListener("click", () => {
    void startPairing();
  });
  document.querySelector("#detect-browser")?.addEventListener("click", () => {
    void detectBrowser();
  });
  document.querySelector("#launch-browser-test")?.addEventListener("click", () => {
    void launchBrowserTest();
  });
  document.querySelector("#close-browser-test")?.addEventListener("click", () => {
    void closeBrowserTest();
  });
}

async function startPairing() {
  pairingActionError = null;
  console.debug("[pairing] Start pairing button clicked");

  try {
    const result = await invoke<PairingState>("start_pairing_session", {
      deviceName: null,
    });
    pairingActionError = null;
    await refreshWith(result);
  } catch (err) {
    pairingActionError = err instanceof Error ? err.message : String(err);
    await refresh();
  }
}

async function detectBrowser() {
  browserActionError = null;
  try {
    await invoke<BrowserRuntimeSnapshot>("detect_browser_runtime");
    await refresh();
  } catch (err) {
    browserActionError = err instanceof Error ? err.message : String(err);
    await refresh();
  }
}

async function launchBrowserTest() {
  browserActionError = null;
  try {
    await invoke<BrowserRuntimeSnapshot>("browser_test_launch");
    await refresh();
  } catch (err) {
    browserActionError = err instanceof Error ? err.message : String(err);
    await refresh();
  }
}

async function closeBrowserTest() {
  browserActionError = null;
  try {
    await invoke<BrowserRuntimeSnapshot>("browser_test_close");
    await refresh();
  } catch (err) {
    browserActionError = err instanceof Error ? err.message : String(err);
    await refresh();
  }
}

async function refreshWith(pairing: PairingState) {
  const [status, browser] = await Promise.all([
    invoke<ConnectorStatus>("get_status"),
    invoke<BrowserRuntimeSnapshot>("get_browser_runtime_status"),
  ]);
  render(status, pairing, browser);
}

async function refresh() {
  try {
    const [status, pairing, browser] = await Promise.all([
      invoke<ConnectorStatus>("get_status"),
      invoke<PairingState>("get_pairing_state"),
      invoke<BrowserRuntimeSnapshot>("get_browser_runtime_status"),
    ]);
    render(status, pairing, browser);
  } catch (err) {
    app.innerHTML = `<p class="error">Failed to load status: ${String(err)}</p>`;
  }
}

async function main() {
  await refresh();
  await listen("connector://status-changed", () => {
    void refresh();
  });
  await listen("connector://pairing-changed", () => {
    void refresh();
  });
  await listen("connector://browser-changed", () => {
    void refresh();
  });

  setInterval(() => {
    void refresh();
  }, 5_000);
}

void main();
