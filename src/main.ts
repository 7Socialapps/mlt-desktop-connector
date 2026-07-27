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

const app = document.querySelector<HTMLDivElement>("#app")!;
let pairingActionError: string | null = null;

function render(status: ConnectorStatus, pairing: PairingState) {
  const displayError = pairing.error ?? pairingActionError;

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

async function refreshWith(pairing: PairingState) {
  const status = await invoke<ConnectorStatus>("get_status");
  render(status, pairing);
}

async function refresh() {
  try {
    const [status, pairing] = await Promise.all([
      invoke<ConnectorStatus>("get_status"),
      invoke<PairingState>("get_pairing_state"),
    ]);
    render(status, pairing);
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

  setInterval(() => {
    void refresh();
  }, 5_000);
}

void main();
