# MLT Desktop Connector

Local desktop agent for MLT Web Poster. Runs in the system tray, maintains a persistent browser session (future milestones), and executes Facebook Marketplace posting jobs via the shared `browser-connector` edge function.

**Milestone A** — foundation scaffold only. No Facebook automation, no production secrets, no remote GitHub repository yet.

---

## Repository setup report

| Field | Value |
|---|---|
| **Local path** | `/Users/drew/Documents/GitHub/mlt-desktop-connector` |
| **Current branch** | `feature/mlt-desktop-connector` |
| **Intended visibility** | Private |
| **Intended org** | [7Socialapps](https://github.com/7Socialapps) |
| **Default branch (future remote)** | `main` |
| **Collaborators** | TBD |
| **CI** | TBD (GitHub Actions: lint, `cargo test`, Tauri build matrix) |
| **Signing secrets** | TBD — Windows Authenticode + macOS Developer ID/notarization stored in org secrets, not in repo |

---

## Pairing flow recommendation

### Selected approach: browser-based pairing with one-time code

**Proposed actions (backend Milestone B):**

1. Desktop calls `create_pairing_code` with `deviceId`, `connectorVersion`, `os`, `capabilities`.
2. Backend returns a short-lived alphanumeric code (e.g. 8 chars, 10-minute TTL).
3. User signs into the MLT dashboard (existing Neon JWT session — same auth as the rest of MLT).
4. Dashboard Web Poster setup UI displays an "Enter pairing code" field and calls `exchange_pairing_code` with the code + authenticated user's `userId` / `dealershipId`.
5. Backend validates the code, binds the device to the user/dealership, issues `accessToken` + `refreshToken`, and invalidates the code.
6. Desktop polls `exchange_pairing_code` **or** receives tokens via a desktop-local callback URL (optional enhancement) — initial implementation can poll with the code until paired.

**Why not embed Neon login in the desktop app?**

- MLT auth is centralized in **Neon JWT** (`neon_session_token`, verified by `verifyNeonSessionToken` in `mlt/supabase/functions/_shared/neonAuth.ts`). The dashboard stores this token after login; edge functions accept `x-neon-session-token`.
- Embedding username/password collection in a desktop tray app increases phishing risk, complicates MFA/password-reset flows, and duplicates the dashboard login surface.
- Phase 1 admin gate (`requirePhase1SuperAdmin`) already expects a valid Neon session for dashboard-initiated actions — browser pairing reuses that trust model without new credential types.

**Why not direct `register_device` from desktop with pasted Neon token?**

- The current `register_device` action requires the caller to present a Neon session header (`requirePhase1SuperAdminDealership`). Pasting JWTs into a desktop app is error-prone and trains bad security habits.
- One-time codes are user-friendly ("enter this code in your dashboard"), time-bound, and auditable. They avoid long-lived session tokens crossing clipboard boundaries.

**Why this does not collect Facebook passwords**

- Facebook login remains inside the local Chromium profile (future Playwright milestone). Pairing only establishes MLT device identity — consistent with `docs/mlt-desktop-connector-contract.md`.

**Fallback until backend pairing lands:** typed client includes stub methods for `create_pairing_code` / `exchange_pairing_code`; existing `register_device` + Neon session remains documented for manual staging tests only.

---

## Local repo structure

```
mlt-desktop-connector/
├── README.md
├── package.json              # Vite + Tauri CLI (frontend)
├── vite.config.ts
├── tsconfig.json
├── index.html
├── .env.staging.example      # Staging env template (no production defaults)
├── src/
│   ├── main.ts               # Minimal status window UI
│   ├── styles.css
│   └── lib/api/
│       ├── types.ts          # Typed contract (mirrors browser-connector)
│       ├── client.ts         # TypeScript HTTP client
│       └── index.ts
└── src-tauri/
    ├── Cargo.toml            # Rust deps: tauri, keyring, tracing, reqwest
    ├── tauri.conf.json
    ├── capabilities/default.json
    ├── icons/
    └── src/
        ├── main.rs
        ├── lib.rs            # Tray, window, lifecycle orchestration
        ├── version.rs        # CONNECTOR_VERSION constant
        ├── config.rs         # Staging-only env config
        ├── device.rs         # UUID device ID persistence
        ├── credentials.rs    # OS keychain via keyring crate
        ├── logging.rs        # tracing + daily rotating file appender
        ├── state.rs
        ├── api/              # Rust typed HTTP client
        ├── services/
        │   ├── heartbeat.rs  # 15s loop, exponential backoff
        │   ├── polling.rs    # Disabled until paired (stub)
        │   └── reconnect.rs  # Sleep/resume + token refresh stub
        └── lifecycle/
            ├── shutdown.rs   # Graceful drain
            ├── single_instance.rs
            └── power.rs      # Sleep/resume gap detection
```

---

## Staging configuration only

The app **refuses to start** unless `MLT_ENV=staging`. There are no production URL or key defaults in code.

Copy the example env file and fill in staging values:

```bash
cp .env.staging.example .env
# Edit .env — use staging Supabase project URL + anon key only
```

| Variable | Required | Description |
|---|---|---|
| `MLT_ENV` | Yes | Must be exactly `staging` |
| `MLT_SUPABASE_URL` | Yes | Staging Supabase project URL |
| `MLT_SUPABASE_ANON_KEY` | Yes | Staging anon/publishable key (not service role) |

**Never commit:** service-role keys, database URLs, device tokens, Facebook credentials, or production secrets.

---

## Development setup

### Prerequisites

- Node.js 20+
- Rust stable (via [rustup](https://rustup.rs))
- Platform deps for Tauri v2 ([Tauri prerequisites](https://v2.tauri.app/start/prerequisites/))

### Commands

```bash
cd /Users/drew/Documents/GitHub/mlt-desktop-connector
npm install
# Load staging env (dotenv is read by Tauri at runtime from process env)
export $(grep -v '^#' .env | xargs)
npm run tauri:dev
```

Build production installer (future, requires signing):

```bash
npm run tauri:build
```

---

## Milestone A capabilities

| # | Capability | Status |
|---|---|---|
| 1 | Git repo on `feature/mlt-desktop-connector` | Done |
| 2 | Tauri v2 scaffold (Rust + TypeScript) | Done |
| 3 | Minimal status window | Done |
| 4 | System tray | Done |
| 5 | Typed API client (TS + Rust) | Done |
| 6 | Staging-only config | Done |
| 7 | OS keychain credential storage | Done |
| 8 | Structured logging + daily rotation | Done |
| 9 | `CONNECTOR_VERSION` reporting | Done (`1.0.0`) |
| 10 | Device ID UUID persistence | Done |
| 11 | Graceful shutdown | Done |
| 12 | Single-instance protection | Done |
| 13 | Sleep/resume handling | Done (clock-gap detector) |
| 14 | Automatic reconnect framework | Stub |
| 15 | Heartbeat loop w/ backoff | Done (skips until paired) |
| 16 | Polling disabled until auth | Done |

---

## Backend contract reference

- Implementation map: `mlt/docs/mlt-desktop-connector-implementation-map.md`
- API contract: `mlt/docs/mlt-desktop-connector-contract.md`
- Edge function: `mlt/supabase/functions/browser-connector/index.ts`

Desktop authenticates with `x-connector-device-token` after pairing. Heartbeat body includes `device_id`, `connector_version`, `os`, `capabilities[]`, `facebook_session_state`.

---

## Rollback plan

Local-only repository — delete the directory or `git checkout` to revert. No deployments or migrations in Milestone A.
