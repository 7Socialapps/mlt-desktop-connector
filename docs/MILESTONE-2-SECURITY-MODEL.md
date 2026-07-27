# Milestone 2 — Security and Privacy Model

This document records the Milestone 2 security/privacy review for MLT Desktop Connector (browser sidecar + Playwright foundation). It describes what data stays local, what reaches the MLT backend, and how failures are redacted.

**Review date:** Milestone 2 close (2.12)  
**Scope:** `credentials/`, `browser/`, `services/heartbeat.rs`, `services/connection_test.rs`, `browser-sidecar/`, `src-tauri/src/lib.rs`

---

## Summary

| Requirement | Status | Notes |
|---|---|---|
| No Facebook passwords collected/stored | **Pass** | Manual login only; detector reads DOM signals, never form values |
| No cookies logged | **Pass** | No cookie read/write/logging in Rust or sidecar |
| No tokens logged | **Pass** | Access tokens in memory only; logs use token *length* at most |
| No browser storage uploaded | **Pass** | Profile, cookies, localStorage stay on disk locally |
| Diagnostic screenshots remain local | **Pass** | Written under `{app_data}/diagnostics/`; not in heartbeat/API |
| Profile directory under app-data only | **Pass** | `{app_data}/browser-profile` via Tauri `app_data_dir()` |
| Credentials separate from browser profile | **Pass** | `{app_data}/credentials.enc` vs `{app_data}/browser-profile/` |
| Profile reset does not delete connector credentials | **Pass** | `reset_profile_dir()` removes profile dir only |
| Connector reset/shutdown stops browser processes | **Pass** | `ShutdownCoordinator` → `BrowserManager::shutdown()` → sidecar `shutdown` |
| Backend receives minimum status metadata only | **Pass** | Heartbeat sends enums/categories, not URLs or screenshots |
| Errors redacted before UI/heartbeat | **Pass** | `sanitize_error()` redacts Bearer tokens and long messages |
| Production build does not expose debug endpoints | **Pass** (after 2.12 fix) | `browser_test_launch` / `browser_test_close` gated in release |

---

## Authentication and credentials

### Connector device credentials

- **Persisted:** Encrypted refresh token + user/dealership IDs in `{app_data}/credentials.enc` (AES-GCM, key in `{app_data}/.credential_key`).
- **In memory only:** Access token — never written to disk (`credentials/types.rs`).
- **Separate from browser:** Credential files live alongside but outside `browser-profile/`.

### Facebook credentials

- **Not collected.** The connector opens Facebook in a persistent Chromium profile; the user types credentials directly into Facebook's UI.
- **`facebook-detector.mjs`** inspects page structure (nav, login form presence, checkpoint text) — not input values or cookies.

---

## Browser profile and local storage

| Path (under Tauri app data) | Contents | Uploaded? |
|---|---|---|
| `browser-profile/` | Chromium user data (cookies, localStorage, cache) | **No** |
| `browser-profile/.profile.lock` | PID lock file (JSON) | **No** |
| `diagnostics/` | Failure screenshots (PNG) | **No** |
| `credentials.enc` | Encrypted device refresh token | **No** (local only) |

Profile resolution: `browser/profile.rs` → `resolve_profile_dir()` → `app_data_dir().join("browser-profile")`.

Profile reset (`BrowserManager::reset_profile`) calls `reset_profile_dir()` only. It does not invoke `credentials::clear_credentials()`.

---

## Backend telemetry (heartbeat)

`HeartbeatRequest` (`api/types.rs`) sends:

- `connector_status`, `browser_status`, `profile_status`, `facebook_session_state`, `marketplace_status` — snake_case enum strings
- `current_browser_url_category` — coarse category from `url_category()` (`facebook_marketplace`, `facebook_auth`, `facebook_other`, `blank`, `unknown`, `other`) — **never the full URL**
- `browser_version` (Playwright version string), optional `last_browser_error_code`, optional `last_browser_check_at`
- Standard pairing fields: `deviceId`, `userId`, `dealershipId`, `connectorVersion`, `os`, `capabilities`

**Not sent:** full URLs, page titles, cookies, screenshots, profile paths, Chromium paths, Facebook DOM content.

Build path: `build_heartbeat_browser_fields()` in `services/connection_test.rs` → `HeartbeatService::send_heartbeat()`.

---

## Logging

| Area | Behavior |
|---|---|
| Access/refresh tokens | Never logged; job claim logs `scoped_token_len` only |
| Cookies | Not referenced in logging |
| Sidecar stderr | Logged at `debug` level (`sidecar.rs`) — Playwright diagnostics, not page content |
| Heartbeat/API errors | `sanitize_error()` strips `Bearer …` and messages >300 chars |
| Facebook detection | Sidecar emits state + `reason_code` events, not credentials |

Log files: `{app_log_dir}/connector.log` (daily rotation). User can open via **Open Log Folder** — logs remain local.

---

## Diagnostic screenshots

On Marketplace navigation failure, `server.mjs` → `captureDiagnosticScreenshot()` writes PNG files to `MLT_BROWSER_DIAGNOSTICS_DIR` (set by Rust to `{app_data}/diagnostics`).

- Path returned in local `MarketplaceSnapshot.screenshot_path` for UI remediation hints
- **Not** included in heartbeat or edge-function payloads

---

## Process lifecycle

| Event | Browser/sidecar behavior |
|---|---|
| Graceful quit (tray) | `ShutdownCoordinator::graceful_shutdown` stops health monitor, calls `BrowserManager::shutdown()` (stop browser + stop sidecar daemon) |
| Sidecar SIGTERM/SIGINT | `teardownBrowser()` closes context, removes lock file, exits |
| Profile lock | `.profile.lock` with live PID → `profile_locked`; stale PID ignored |

---

## Production vs development

| Surface | Development | Production (release) |
|---|---|---|
| `browser_test_launch` / `browser_test_close` | Available (Playwright CLI smoke tests) | **Rejected** — returns error (2.12 gate) |
| `npm run browser:test` | Dev-only CLI script | Not bundled in installer (2.13) |
| System Node / project folder | Dev sidecar spawn | Bundled sidecar in packaged app (planned 2.13) |

Release builds use `profile.release` with `strip = true` and `panic = abort`.

---

## Test Connection (local diagnostics)

`ConnectionTestReport` is built locally in `run_connection_tests()` and returned to the desktop UI via Tauri invoke. It is **not** uploaded as a blob to the backend during Milestone 2.

Checks report pass/warn/fail with user-facing labels and optional `error_code` — no tokens, cookies, or raw URLs.

---

## Residual risks and mitigations

| Risk | Mitigation / follow-up |
|---|---|
| Full URL visible in local UI (`get_active_page`, browser snapshot) | Acceptable — local operator view only; heartbeat uses categories |
| Chromium path logged at `info` during runtime detect | Path metadata only; no session data |
| Facebook DOM selectors break | Detector uses multi-signal fallback; no security regression |
| Packaged sidecar must not depend on dev machine Node | Addressed in packaging plan (2.13) |

---

## 2.12 code fix

**Debug command gating:** `browser_test_launch` and `browser_test_close` in `lib.rs` now return an error when compiled without `debug_assertions` (release/profile release builds).

---

## Verification checklist for operators

1. Pair device — confirm `credentials.enc` appears under app data, not inside `browser-profile/`.
2. Sign into Facebook manually — confirm no password fields are read by connector code (network tab / logs).
3. Trigger heartbeat — inspect payload (staging proxy/logs): snake_case status fields only, `current_browser_url_category` not a full URL.
4. Fail Marketplace navigation — confirm PNG under `diagnostics/`, not in API traffic.
5. Reset browser profile — confirm `credentials.enc` remains; Facebook session cleared.
6. Quit connector — confirm no orphaned Chromium/sidecar processes (`ps` / Activity Monitor).
7. Release build — invoke `browser_test_launch` from devtools/console; expect rejection.
