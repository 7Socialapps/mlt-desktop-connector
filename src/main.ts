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
  phase:
    | "idle"
    | "checking"
    | "downloading"
    | "ready_to_install"
    | "install_stalled"
    | "error";
  message: string;
  available_version: string | null;
  progress: number;
  error: string | null;
  installer_path: string | null;
  timed_out: boolean;
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

interface RuntimeLocation {
  from_dmg_volume: boolean;
  from_applications: boolean;
  exe_path: string;
}

/** Hard caps — dealers must never see infinite Please wait / Force Quit. */
const SETUP_UI_MAX_MS = 45_000;
const CONNECTING_UI_MAX_MS = 45_000;

const app = document.querySelector<HTMLDivElement>("#app")!;
let actionInProgress: string | null = null;
let actionError: string | null = null;
/** Non-error dealer guidance (e.g. after Open Facebook). */
let actionHint: string | null = null;
let showAbout = false;

const FACEBOOK_LOGIN_HINT =
  "A Chrome window will open — sign into Facebook like you normally do. You only need to do this once on this computer. If asked for a security key, choose Try another way → password or text code.";

const CHROME_INSTALL_HINT =
  "Install Google Chrome for easiest Facebook login: https://www.google.com/chrome/ — then click Open Facebook again. Do not use Safari.";

function looksLikeSystemChrome(browser: BrowserManagerSnapshot): boolean {
  const p = (browser.chromium_path || "").toLowerCase();
  return (
    p.includes("google chrome") ||
    p.includes("chrome.exe") ||
    p.includes("microsoft edge") ||
    p.includes("msedge")
  );
}
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
  installer_path: null,
  timed_out: false,
};
let runtimeLocation: RuntimeLocation = {
  from_dmg_volume: false,
  from_applications: true,
  exe_path: "",
};
let setupStartedAt: number | null = null;
let connectingStartedAt: number | null = null;
let forceNotConnected = false;

type DealerView =
  | { kind: "dmg_gate"; subtitle: string }
  | { kind: "starting"; subtitle: string }
  | { kind: "setup"; subtitle: string; progress?: number }
  | {
      kind: "updating";
      subtitle: string;
      progress?: number;
      mode: "downloading" | "ready" | "stalled";
    }
  | { kind: "connecting"; subtitle: string }
  | { kind: "not_connected"; subtitle: string; cta: "open_facebook" | "retry" | "wait" }
  | { kind: "connected"; subtitle: string };

function isFacebookLoggedIn(browser: BrowserManagerSnapshot, runtime: RuntimeStatus): boolean {
  return (
    runtime.facebook_session_state === "logged_in" ||
    browser.facebook_session.state === "facebook_logged_in"
  );
}

function setupTimedOut(): boolean {
  if (setupStartedAt == null) return false;
  return Date.now() - setupStartedAt >= SETUP_UI_MAX_MS;
}

function connectingTimedOut(): boolean {
  if (connectingStartedAt == null) return false;
  return Date.now() - connectingStartedAt >= CONNECTING_UI_MAX_MS;
}

function isUpdateFinishablePhase(phase: UpdateUiState["phase"]): boolean {
  return phase === "ready_to_install" || phase === "install_stalled";
}

function computeDealerView(
  status: ConnectorStatus,
  pairing: PairingState,
  browser: BrowserManagerSnapshot,
  runtime: RuntimeStatus,
): DealerView {
  // Running from the DMG mount — show once, never download another update.
  if (runtimeLocation.from_dmg_volume) {
    return {
      kind: "dmg_gate",
      subtitle: "Drag this app to Applications, then open it from there.",
    };
  }

  // Only show update UI when an update is actually active / finishable.
  // Never paint Updating for idle/checking when already on latest.
  if (
    updateState.active ||
    isUpdateFinishablePhase(updateState.phase) ||
    updateState.phase === "downloading"
  ) {
    const stalled =
      updateState.phase === "install_stalled" || updateState.timed_out;
    const ready = isUpdateFinishablePhase(updateState.phase);
    const subtitle =
      updateState.message ||
      (stalled
        ? "Still on the old version. Finish installing, or open the installer again."
        : ready
          ? "Installer open — drag to Applications, then open it from Applications."
          : "Updating…");
    return {
      kind: "updating",
      subtitle,
      progress:
        updateState.phase === "downloading" ? updateState.progress : undefined,
      mode: stalled ? "stalled" : ready ? "ready" : "downloading",
    };
  }

  const inSetup =
    !forceNotConnected &&
    (status.connection_state === "starting" || chromiumProvision.active);
  if (inSetup) {
    if (setupStartedAt == null) setupStartedAt = Date.now();
    if (!setupTimedOut()) {
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
    forceNotConnected = true;
    if (!actionError) {
      actionError =
        chromiumProvision.error ||
        "Setup is taking too long. Click Try again, or click Connect in MLT on the web.";
    }
  } else if (!chromiumProvision.active && status.connection_state !== "starting") {
    setupStartedAt = null;
  }

  // Auto-pair in progress from dashboard Connect — never show a pairing code.
  if (
    !forceNotConnected &&
    !status.paired &&
    (pairing.active || pairing.status === "connecting")
  ) {
    if (connectingStartedAt == null) connectingStartedAt = Date.now();
    if (!connectingTimedOut()) {
      return {
        kind: "connecting",
        subtitle: status.deep_link_message ?? "Connecting… Keep this app open.",
      };
    }
    if (!actionError) {
      actionError =
        pairing.error ||
        "Connecting timed out. Click Connect in MLT on the web again.";
    }
  } else if (status.paired || (!pairing.active && pairing.status !== "connecting")) {
    connectingStartedAt = null;
  }

  if (status.needs_reconnect) {
    return {
      kind: "not_connected",
      subtitle:
        status.deep_link_message ??
        "This computer was disconnected. Click Connect in MLT on the web to link it again.",
      cta: "open_facebook",
    };
  }

  if (!browser.enabled) {
    return {
      kind: "not_connected",
      subtitle: "Facebook helper is turned off on this computer. Contact support.",
      cta: "retry",
    };
  }

  // Unpaired: still allow Open Facebook so dealers can sign into FB while Connect pairs MLT.
  if (!status.paired) {
    return {
      kind: "not_connected",
      subtitle:
        status.deep_link_message ??
        "Click Connect in MLT on the web to link this computer — or Open Facebook to sign in.",
      cta: "open_facebook",
    };
  }

  if (!browser.chromium_installed || !browser.sidecar_running) {
    return {
      kind: "not_connected",
      subtitle:
        "Almost ready — click Open Facebook to finish browser setup (downloads once if needed).",
      cta: "open_facebook",
    };
  }

  if (!isFacebookLoggedIn(browser, runtime)) {
    const chromeReady = looksLikeSystemChrome(browser);
    return {
      kind: "not_connected",
      subtitle:
        status.deep_link_message ??
        (chromeReady
          ? "Click Open Facebook — a normal Chrome window opens. Sign in like you usually do (once on this computer). If asked for a security key: Try another way → password or text code."
          : "For easiest Facebook login, install Google Chrome (google.com/chrome), then click Open Facebook. Do not use Safari."),
      cta: "open_facebook",
    };
  }

  return {
    kind: "connected",
    subtitle:
      "You’re connected. Keep this app running while you post from MLT. Facebook stays in the Connector’s Chrome window — not Safari.",
  };
}

function render(
  status: ConnectorStatus,
  pairing: PairingState,
  browser: BrowserManagerSnapshot,
  runtime: RuntimeStatus,
) {
  const view = computeDealerView(status, pairing, browser, runtime);
  const busy = Boolean(actionInProgress);
  const connected = view.kind === "connected";

  const statusTitle =
    view.kind === "connected"
      ? "Connected"
      : view.kind === "dmg_gate"
        ? "Finish installing"
        : view.kind === "updating"
          ? view.mode === "ready" || view.mode === "stalled"
            ? "Installer open"
            : "Updating…"
          : view.kind === "connecting"
            ? "Connecting…"
            : view.kind === "setup" || view.kind === "starting"
              ? "Setting up…"
              : "Not connected";

  const updateActions =
    view.kind === "updating" && (view.mode === "ready" || view.mode === "stalled")
      ? `<button id="finish-install" class="dealer-primary" ${busy ? "disabled" : ""}>I've finished installing</button>
         ${
           view.mode === "stalled"
             ? `<button id="reopen-installer" class="dealer-secondary" ${busy ? "disabled" : ""}>Open installer again</button>
                <button id="retry-update" class="dealer-secondary" ${busy ? "disabled" : ""}>Retry</button>`
             : `<button id="reopen-installer" class="dealer-secondary" ${busy ? "disabled" : ""}>Open installer again</button>`
         }`
      : view.kind === "updating"
        ? `<button class="dealer-primary" disabled>Updating…</button>`
        : "";

  const primaryActions =
    view.kind === "dmg_gate"
      ? `<button id="quit-dmg" class="dealer-primary">Quit</button>`
      : view.kind === "updating"
        ? updateActions
        : view.kind === "connected"
          ? `<button id="primary-cta" class="dealer-primary" ${busy ? "disabled" : ""}>Open Facebook</button>`
          : view.kind === "setup" ||
              view.kind === "starting" ||
              view.kind === "connecting"
            ? `<button class="dealer-primary" disabled>${
                view.kind === "connecting" ? "Connecting…" : "Please wait…"
              }</button>`
            : view.kind === "not_connected" && view.cta === "open_facebook"
              ? `<button id="primary-cta" class="dealer-primary" ${busy ? "disabled" : ""}>${
                  busy ? "Opening Facebook…" : "Open Facebook"
                }</button>`
              : view.kind === "not_connected" && view.cta === "retry"
                ? `<button id="primary-cta" class="dealer-primary" ${busy ? "disabled" : ""}>Try again</button>`
                : `<button class="dealer-primary" disabled>Waiting for Connect…</button>`;

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
        actionError
          ? `<p class="error" role="alert">${actionError}</p>`
          : ""
      }

      ${
        actionHint
          ? `<p class="dealer-hint" role="status">${actionHint}</p>`
          : ""
      }

      <div class="dealer-actions">
        ${primaryActions}
      </div>

      <p class="dealer-footnote">
        ${
          view.kind === "dmg_gate"
            ? "Do not download another copy. Quit this window, then open the app from Applications."
            : view.kind === "updating" && (view.mode === "ready" || view.mode === "stalled")
              ? "Use the installer window to drag the app into Applications, then click I’ve finished installing."
              : view.kind === "not_connected" && view.cta === "open_facebook"
                ? looksLikeSystemChrome(browser)
                  ? "Opens real Google Chrome with a saved MLT login profile on this Mac. Not Safari."
                  : "Install Google Chrome first for Touch ID / easy 2FA: https://www.google.com/chrome/"
                : "You can close this window — the connector stays in your menu bar."
        }
      </p>

      <details class="dealer-about" ${showAbout ? "open" : ""}>
        <summary>About</summary>
        <p class="helper">Version ${status.connector_version}</p>
        <p class="helper">Browser: ${
          looksLikeSystemChrome(browser)
            ? "Google Chrome (recommended)"
            : "Bundled fallback — install Google Chrome for easiest login"
        }</p>
        <p class="helper">Facebook login profile (persists): ${
          browser.profile_path
            ? browser.profile_path
            : "~/Library/Application Support/com.7socialapps.mlt-desktop-connector/browser-profile"
        }</p>
        <p class="helper">Same Chrome profile is used for Marketplace posting. Never use Safari for this session.</p>
        <button id="open-log-folder" type="button" class="dealer-linkish">Open logs folder</button>
      </details>
    </main>
  `;

  bindActions(view);
}

function bindActions(view: DealerView) {
  document.querySelector("details.dealer-about")?.addEventListener("toggle", (e) => {
    showAbout = (e.target as HTMLDetailsElement).open;
  });
  document.querySelector("#open-log-folder")?.addEventListener("click", () => void openLogFolder());
  document.querySelector("#finish-install")?.addEventListener("click", () => void finishInstall());
  document.querySelector("#reopen-installer")?.addEventListener("click", () => void reopenInstaller());
  document.querySelector("#retry-update")?.addEventListener("click", () => void retryUpdate());
  document.querySelector("#quit-dmg")?.addEventListener("click", () => void quitApp());
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
      forceNotConnected = false;
      setupStartedAt = null;
      connectingStartedAt = null;
      actionError = null;
      void refresh();
    }
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

async function quitApp() {
  try {
    await invoke("quit_app");
  } catch (err) {
    console.error(err);
    window.close();
  }
}

async function finishInstall() {
  await withAction("finish_install", async () => {
    try {
      await invoke("finish_update_install");
      actionError =
        "If the app didn’t reopen, open MLT Desktop Connector from Applications (not the installer window).";
    } catch (err) {
      actionError =
        typeof err === "string"
          ? err
          : "Drag the app to Applications first, then click I’ve finished installing.";
      console.error(err);
    }
  });
}

async function reopenInstaller() {
  await withAction("reopen_installer", async () => {
    try {
      await invoke("reopen_update_installer");
    } catch (err) {
      actionError =
        typeof err === "string"
          ? err
          : "Couldn’t open the installer. Click Retry to download again.";
      console.error(err);
    }
  });
}

async function retryUpdate() {
  await withAction("retry_update", async () => {
    try {
      actionError = null;
      await invoke("check_for_updates");
    } catch (err) {
      actionError = "Couldn’t check for updates. Try again in a moment.";
      console.error(err);
    }
  });
}

function invokeErrorMessage(err: unknown): string {
  if (typeof err === "string") return err;
  if (err instanceof Error) return err.message;
  if (err && typeof err === "object") {
    const o = err as Record<string, unknown>;
    if (typeof o.message === "string" && o.message.trim()) return o.message;
    if (typeof o.error === "string" && o.error.trim()) return o.error;
    try {
      return JSON.stringify(err);
    } catch {
      /* fall through */
    }
  }
  return String(err ?? "");
}

function dealerFacebookError(err: unknown): string {
  const raw = invokeErrorMessage(err);
  const msg = raw.replace(/^Error:\s*/i, "").trim();
  if (!msg || msg === "[object Object]") {
    return "Couldn’t open Facebook. Click Open Facebook to try again.";
  }
  // Always surface the real helper/Playwright error — dealers cannot debug a blank CTA.
  if (msg.includes("xattr") || msg.includes("Gatekeeper") || msg.includes("quarantine")) {
    return msg.length > 400 ? `${msg.slice(0, 380)}…` : msg;
  }
  if (
    msg.includes("SyntaxError") ||
    msg.includes("Unexpected token") ||
    msg.includes("ERR_MODULE_NOT_FOUND")
  ) {
    return msg.length > 400
      ? `${msg.slice(0, 380)}…`
      : `${msg} — reinstall Desktop Connector v1.1.4+.`;
  }
  if (
    msg.includes("missing") ||
    msg.includes("reinstall") ||
    msg.includes("ERR_MODULE") ||
    msg.includes("not installed")
  ) {
    return "Browser components are missing. Quit this app and install the latest Desktop Connector (v1.1.4+), then try Open Facebook again.";
  }
  if (msg.includes("timed out") || msg.includes("timeout") || msg.includes("network")) {
    return msg.length > 400
      ? `${msg.slice(0, 380)}…`
      : "Browser setup timed out. Check your network, then click Open Facebook again.";
  }
  if (msg.length > 400) {
    return `${msg.slice(0, 380)}…`;
  }
  return msg;
}

async function openFacebookLogin() {
  await withAction("open_facebook", async () => {
    try {
      actionError = null;
      actionHint = null;
      // Provisions if needed, recovers sidecar, opens facebook.com in real Chrome when installed
      const snap = await invoke<BrowserManagerSnapshot>("browser_open_facebook_login");
      actionHint = looksLikeSystemChrome(snap)
        ? FACEBOOK_LOGIN_HINT
        : `${FACEBOOK_LOGIN_HINT} ${CHROME_INSTALL_HINT}`;
    } catch (err) {
      actionError = dealerFacebookError(err);
      actionHint = FACEBOOK_LOGIN_HINT;
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
    const [status, pairing, browser, runtime, provision, update, location] =
      await Promise.all([
        invoke<ConnectorStatus>("get_status"),
        invoke<PairingState>("get_pairing_state"),
        invoke<BrowserManagerSnapshot>("get_browser_status"),
        invoke<RuntimeStatus>("runtime_status"),
        invoke<ChromiumProvisionState>("get_chromium_provision_state"),
        invoke<UpdateUiState>("get_update_state"),
        invoke<RuntimeLocation>("get_runtime_location"),
      ]);
    chromiumProvision = provision;
    runtimeLocation = location;
    updateState = {
      ...update,
      installer_path: update.installer_path ?? null,
      timed_out: update.timed_out ?? false,
    };
    if (update.phase === "error" && update.message && !actionError) {
      actionError = update.message;
    }
    if (provision.error && !actionError && !provision.active) {
      actionError = provision.error;
    }
    if (pairing.error && !actionError && pairing.status === "error") {
      actionError = pairing.error;
    }
    render(status, pairing, browser, runtime);
  } catch (err) {
    app.innerHTML = `<section class="dealer-shell"><h1>MLT Desktop Connector</h1><p class="error">Couldn’t load status. Keep the app open and try again in a moment.</p><div class="dealer-actions"><button id="retry-load" class="dealer-primary">Try again</button></div></section>`;
    document.querySelector("#retry-load")?.addEventListener("click", () => void refresh());
    console.error(err);
  }
}

async function main() {
  const refreshSafe = () => {
    void refresh().catch(() => {
      app.innerHTML = `<section class="dealer-shell"><h1>MLT Desktop Connector</h1><p class="error">Still starting…</p><div class="dealer-actions"><button id="retry-load" class="dealer-primary">Try again</button></div></section>`;
      document.querySelector("#retry-load")?.addEventListener("click", () => void refresh());
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
      updateState = {
        ...event.payload,
        installer_path: event.payload.installer_path ?? null,
        timed_out: event.payload.timed_out ?? false,
      };
      refreshSafe();
    }),
  ]).catch(() => {
    app.innerHTML = `<section class="dealer-shell"><h1>MLT Desktop Connector</h1><p class="error">Couldn’t start the window. Quit and reopen the app.</p></section>`;
  });

  window.setTimeout(refreshSafe, 0);
  setInterval(refreshSafe, 2_000);
}

void main();
