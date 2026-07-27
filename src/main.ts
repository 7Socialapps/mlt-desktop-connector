import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import "./styles.css";

interface ConnectorStatus {
  device_id: string;
  connector_version: string;
  environment: string;
  paired: boolean;
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

function render(status: ConnectorStatus, pairing: PairingState) {
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
        <div><dt>Last heartbeat</dt><dd>${status.last_heartbeat_at ?? "—"}</dd></div>
        ${
          status.last_error
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
        <p class="helper">Status: ${pairing.status}${pairing.error ? ` — ${pairing.error}` : ""}</p>
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
    void invoke("start_pairing_session", { deviceName: null }).then(refresh);
  });
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
