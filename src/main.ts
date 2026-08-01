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

interface UpdateUiState {
  active: boolean;
  phase: "idle" | "checking" | "downloading" | "ready_to_install" | "error";
  message: string;
  available_version: string | null;
  progress: number;
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

const app = document.querySelector<HTMLDivElement>("#app")!;
let actionInProgress: string | null = null;
let actionError: string | null = null;
let showAbout = false;
let chromiumProvision: ChromiumProvisionState = {
  active: false,
  progress: 0,
  message: "Setting up…",
  error: null,
};
let updateState: UpdateUiState = {
  active: false,
  phase: "idle",
  message: "",
  available_version: null,
  progress: 0,
  error: null,
};

type DealerView =
  | { kind: "starting"; subtitle: string }
  | { kind: "setup"; subtitle: string; progress?: number }
  | { kind: "updating"; subtitle: string; progress?: number }
  | { kind: "not_connected"; subtitle: string; cta: "connect" | "open_facebook" | "retry" }
  | { kind: "connected"; subtitle: string };

function isFacebookLoggedIn(browser: BrowserManagerSnapshot, runtime: RuntimeStatus): boolean {
  return (
    runtime.facebook_session_state === "logged_in" ||
    browser.facebook_session.state === "facebook_logged_in"
  );
}

function computeDealerView(
  status: ConnectorStatus,
  browser: BrowserManagerSnapshot,
  runtime: RuntimeStatus,
): DealerView {
  if (updateState.active || updateState.phase === "ready_to_install") {
    const subtitle =
      updateState.message ||
      (updateState.phase === "ready_to_install"
        ? "Update ready — finish installing, then reopen the app."
        : "Updating…");
    return {
      kind: "updating",
      subtitle,
      progress:
        updateState.phase === "downloading" || updateState.phase === "checking"
          ? updateState.progress
          : undefined,
    };
  }

  if (status.connection_state === "starting" || chromiumProvision.active) {
    if (chromiumProvision.active) {
      return {
        kind: "setup",
        subtitle: "Finishing a one-time setup. This can take a minute.",
        progress: chromiumProvision.progress,
      };
    }
    return {
      kind: "starting",
      subtitle: "Starting up…",
    };
  }

  if (status.needs_reconnect || !status.paired) {
    return {
      kind: "not_connected",
      subtitle:
        status.deep_link_message ??
        "Click Connect in MLT on the web to link this computer. Keep this app open.",
      cta: "connect",
    };
  }

  if (!browser.enabled) {
    return {
      kind: "not_connected",
      subtitle: "Facebook helper is turned off on this computer. Contact support.",
      cta: "retry",
    };
  }

  if (!browser.chromium_installed || !browser.sidecar_running) {
    return {
      kind: "not_connected",
      subtitle:
        "Almost ready — click Connect in MLT on the web, or Open Facebook below to finish setup.",
      cta: "open_facebook",
    };
  }

  if (!isFacebookLoggedIn(browser, runtime)) {
    return {
      kind: "not_connected",
      subtitle:
        status.deep_link_message ??
        "Sign into Facebook to finish connecting. Your password stays with Facebook.",
      cta: "open_facebook",
    };
  }

  return {
    kind: "connected",
    subtitle: "You’re connected. Keep this app running while you post from MLT.",
  };
}

function render(
  status: ConnectorStatus,
  pairing: PairingState,
  browser: BrowserManagerSnapshot,
  runtime: RuntimeStatus,
) {
  const view = computeDealerView(status, browser, runtime);
  const busy = Boolean(actionInProgress);
  const connected = view.kind === "connected";
  const primaryLabel =
    view.kind === "not_connected"
      ? view.cta === "open_facebook"
        ? "Open Facebook"
        : view.cta === "retry"
          ? "Try again"
          : "Connect"
      : "Open Facebook";

  const statusTitle =
    view.kind === "connected"
      ? "Connected"
      : view.kind === "updating"
        ? "Updating…"
        : view.kind === "setup" || view.kind === "starting"
          ? "Setting up…"
          : "Not connected";

  app.innerHTML = `
    <main class="dealer-shell">
      <header class="dealer-header">
        <h1>MLT Desktop Connector</h1>
      </header>

      <section class="dealer-status ${connected ? "is-connected" : "is-offline"}" aria-live="polite">
        <div class="dealer-status-dot" aria-hidden="true"></div>
        <div class="dealer-status-copy">
          <p class="dealer-status-title">${statusTitle}</p>
          <p class="dealer-status-sub">${view.subtitle}</p>
        </div>
      </section>

      ${
        (view.kind === "setup" || view.kind === "updating") &&
        typeof view.progress === "number"
          ? `<div class="progress-bar" role="progressbar" aria-valuenow="${view.progress}" aria-valuemin="0" aria-valuemax="100">
              <div class="progress-fill" style="width:${view.progress}%"></div>
            </div>`
          : ""
      }

      ${
        pairing.active && pairing.pairing_code
          ? `<section class="dealer-pairing" role="status">
              <p class="dealer-status-sub">Connecting… enter this code in MLT if asked:</p>
              <div class="pairing-code">${pairing.pairing_code}</div>
            </section>`
          : ""
      }

      ${
        actionError
          ? `<p class="error" role="alert">${actionError}</p>`
          : ""
      }

      <div class="dealer-actions">
        ${
          view.kind === "connected"
            ? `<button id="primary-cta" class="dealer-primary" ${busy ? "disabled" : ""}>Open Facebook</button>`
            : view.kind === "setup" ||
                view.kind === "starting" ||
                view.kind === "updating"
              ? `<button class="dealer-primary" disabled>${
                  view.kind === "updating" ? "Updating…" : "Please wait…"
                }</button>`
              : `<button id="primary-cta" class="dealer-primary" ${busy ? "disabled" : ""}>${
                  busy ? "Working…" : primaryLabel
                }</button>`
        }
      </div>

      <p class="dealer-footnote">
        You can close this window — the connector stays in your menu bar.
      </p>

      <details class="dealer-about" ${showAbout ? "open" : ""}>
        <summary>About</summary>
        <p class="helper">Version ${status.connector_version}</p>
        <button id="open-log-folder" type="button" class="dealer-linkish">Open logs folder</button>
      </details>
    </main>
  `;

  bindActions(view, status);
}

function bindActions(view: DealerView, _status: ConnectorStatus) {
  document.querySelector("details.dealer-about")?.addEventListener("toggle", (e) => {
    showAbout = (e.target as HTMLDetailsElement).open;
  });
  document.querySelector("#open-log-folder")?.addEventListener("click", () => void openLogFolder());
  document.querySelector("#primary-cta")?.addEventListener("click", () => {
    if (view.kind === "connected") {
      void openFacebookLogin();
      return;
    }
    if (view.kind !== "not_connected") return;
    if (view.cta === "open_facebook") {
      void openFacebookLogin();
      return;
    }
    if (view.cta === "retry") {
      void refresh();
      return;
    }
    // Not paired — start pairing; dealer finishes via dashboard Connect.
    void startPairing();
  });
}

async function withAction(name: string, fn: () => Promise<void>) {
  actionInProgress = name;
  actionError = null;
  await refresh();
  try {
    await fn();
  } finally {
    actionInProgress = null;
    await refresh();
  }
}

async function startPairing() {
  actionError = null;
  actionInProgress = "connect";
  await refresh();
  try {
    await invoke<PairingState>("start_pairing_session", { deviceName: null });
    actionError = null;
  } catch (err) {
    actionError =
      "Couldn’t start connecting. Click Connect in MLT on the web, or try again.";
    console.error(err);
  } finally {
    actionInProgress = null;
    await refresh();
  }
}

async function openFacebookLogin() {
  await withAction("open_facebook", async () => {
    try {
      // Ensure browser is up, then open Facebook login.
      try {
        await invoke<BrowserManagerSnapshot>("browser_launch");
      } catch {
        /* launch may fail if already running — still try login */
      }
      await invoke<BrowserManagerSnapshot>("browser_open_facebook_login");
    } catch (err) {
      actionError =
        "Couldn’t open Facebook. Click Connect in MLT on the web, then try again.";
      console.error(err);
    }
  });
}

async function openLogFolder() {
  try {
    await invoke("open_log_folder");
  } catch (err) {
    actionError = "Couldn’t open the logs folder.";
    console.error(err);
    await refresh();
  }
}

async function refresh() {
  try {
    const [status, pairing, browser, runtime, provision, update] = await Promise.all([
      invoke<ConnectorStatus>("get_status"),
      invoke<PairingState>("get_pairing_state"),
      invoke<BrowserManagerSnapshot>("get_browser_status"),
      invoke<RuntimeStatus>("runtime_status"),
      invoke<ChromiumProvisionState>("get_chromium_provision_state"),
      invoke<UpdateUiState>("get_update_state"),
    ]);
    chromiumProvision = provision;
    updateState = update;
    if (update.phase === "error" && update.message && !actionError) {
      actionError = update.message;
    }
    render(status, pairing, browser, runtime);
  } catch (err) {
    app.innerHTML = `<section class="dealer-shell"><h1>MLT Desktop Connector</h1><p class="error">Couldn’t load status. Keep the app open and try again in a moment.</p></section>`;
    console.error(err);
  }
}

async function main() {
  const refreshSafe = () => {
    void refresh().catch(() => {
      app.innerHTML = `<section class="dealer-shell"><h1>MLT Desktop Connector</h1><p class="error">Still starting…</p></section>`;
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
    listen<UpdateUiState>("connector://update-changed", (event) => {
      updateState = event.payload;
      refreshSafe();
    }),
  ]).catch(() => {
    app.innerHTML = `<section class="dealer-shell"><h1>MLT Desktop Connector</h1><p class="error">Couldn’t start the window. Quit and reopen the app.</p></section>`;
  });

  window.setTimeout(refreshSafe, 0);
  setInterval(refreshSafe, 5_000);
}

void main();
