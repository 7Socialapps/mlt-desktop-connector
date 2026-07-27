# Shared Facebook Runtime Architecture

The MLT Desktop Connector is the **single local Facebook runtime** for all MLT products that require a signed-in browser session on the dealer's machine — starting with **Web Poster** (Marketplace) and extending to **Leads** (Messenger) without duplicating browser infrastructure.

## Design principles

1. **One Chromium runtime** — Playwright sidecar + `BrowserManager` lifecycle.
2. **One persistent profile** — `{app_data}/browser-profile/` holds Facebook cookies and session state.
3. **One paired device identity** — encrypted credentials in `{app_data}/credentials.enc`.
4. **One heartbeat/status channel** — connector + browser + Facebook + Marketplace fields to the dashboard.
5. **Separate product services** — Marketplace and Messenger automation are distinct services that **reuse** the shared runtime; they do not launch their own browsers or profiles.

## Service boundaries (target)

| Service | Responsibility | Shares |
|---|---|---|
| **BrowserManager** | Chromium launch/stop, profile lock, sidecar daemon, health | Profile path, sidecar process |
| **FacebookSessionService** | Login state detection, session expiry, checkpoint handling | BrowserManager page access |
| **MarketplaceService** | Marketplace navigation, listing form automation, image upload | BrowserManager + FacebookSessionService |
| **MessengerService** | Messenger thread access, lead retrieval (future) | BrowserManager + FacebookSessionService |
| **NotificationService** | Desktop/system notifications for jobs and attention states | App state only |
| **DiagnosticsService** | Screenshots, structured error codes, evidence capture | BrowserManager (on demand) |

## Current implementation (M3.2)

Implemented today under `src-tauri/src/browser/`:

- `BrowserManager` — sidecar RPC, profile, launch/stop
- Facebook session detection — `facebook.rs` + sidecar `facebook-detector.mjs`
- Marketplace status — `marketplace.rs` + sidecar `marketplace-evaluator.mjs`

Marketplace job automation (`src-tauri/src/marketplace/`) builds on the same profile and sidecar. **No MessengerService exists yet** — Leads must not introduce a second browser or profile when added.

## Non-goals

- Do not derive location or identity from browser geolocation, IP, or device GPS.
- Do not store Facebook cookies or refresh tokens in the repository.
- Do not create product-specific browser profiles (Web Poster profile vs Leads profile).

## Milestone alignment

- **M2** — Browser foundation, shared profile, heartbeat contract.
- **M3** — Marketplace automation services on top of BrowserManager.
- **Future Leads** — MessengerService on the same BrowserManager + FacebookSessionService; separate job queue and edge-function contracts, shared connector heartbeat.
