# Shared Facebook Runtime Architecture

The MLT Desktop Connector is the **single local Facebook runtime** for all MLT products that require a signed-in browser session on the dealer's machine — starting with **Web Poster** (Marketplace) and extending to **Leads** (Messenger) without duplicating browser infrastructure.

## Design principles

1. **One Chromium runtime** — Playwright sidecar + `BrowserManager` lifecycle.
2. **One persistent profile** — `{app_data}/browser-profile/` holds Facebook cookies and session state.
3. **One paired device identity** — encrypted credentials in `{app_data}/credentials.enc`.
4. **One heartbeat/status channel** — connector + browser + Facebook + Marketplace + runtime service fields to the dashboard.
5. **Separate product services** — Marketplace, Messenger, and Notifications automation are distinct services that **reuse** the shared runtime via `ServiceBus`; they do not launch their own browsers or profiles.

## Service boundaries

| Service | Responsibility | Shares |
|---|---|---|
| **BrowserManager** | Chromium launch/stop, profile lock, sidecar daemon, health, thin Tauri command wrappers | Profile path, sidecar process |
| **ServiceBus** | Single-browser mutex, routes RPC to BrowserManager, tracks `current_service` | BrowserManager |
| **FacebookSessionService** | Login state detection, session expiry, checkpoint/MFA/restriction/disabled, account identity (display name framework) | ServiceBus → BrowserManager |
| **NavigationService** | Deterministic navigation (`navigate` RPC), retry/timeout, page readiness | ServiceBus → BrowserManager |
| **MarketplaceService** | Marketplace status + navigation to create listing (no form filling in M4) | Session + Navigation via ServiceBus |
| **MessengerService** | Messenger navigation + readiness framework (no scraping in M4) | Navigation via ServiceBus |
| **NotificationService** | Notifications navigation + readiness framework (unread count stub) | Navigation via ServiceBus |
| **RecoveryService** | Crash, redirect, checkpoint, logout, network, restart policies | BrowserManager crash recovery (wrap, not duplicate) |
| **FacebookRuntime** | Aggregates all services; `FacebookRuntimeStatus` for heartbeat | All services above |

## Current implementation (M4)

Implemented under `src-tauri/src/runtime/`:

- `ServiceBus` — internal dispatcher with mutex; services never call BrowserManager directly from outside runtime
- `FacebookSessionService` — sole owner of Facebook session state parsing
- `NavigationService` — destinations enum + sidecar `navigate` RPC
- `MarketplaceService` — `open_marketplace`, `open_create_listing`, delegates from BrowserManager
- `MessengerService` / `NotificationService` — framework only (navigation + state evaluation)
- `FacebookRuntimeStatus` — aggregated heartbeat fields (M4)
- Sidecar: `navigation.mjs`, `messenger-evaluator.mjs`, `notifications-evaluator.mjs`, RPCs `navigate`, `open_messenger`, `open_notifications`

Marketplace job asset pipeline remains in `src-tauri/src/marketplace/assets/` (M3). Form filling/posting is **not** started in M4.

## Non-goals (M4)

- Messenger scraping, AI replies, conversation parsing
- Marketplace form filling or posting (M3.3+)
- Product-specific browser profiles

## Milestone alignment

- **M2** — Browser foundation, shared profile, heartbeat contract.
- **M3** — Marketplace payload/assets on top of BrowserManager.
- **M4** — Shared Facebook runtime services (this document).
- **Future Leads** — MessengerService scraping on same runtime; separate job queue, shared heartbeat.
