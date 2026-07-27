# Milestone 2 — Implementation Plan

| Status | Sub-milestone |
|---|---|
| **Complete** | 2.1 Playwright Runtime Foundation |
| **Complete** | 2.2 Browser Manager |
| **Complete** | 2.3 Persistent Browser Profile |
| **Complete** | 2.4 Manual Facebook Login Flow |
| **Complete** | 2.5 Facebook Session Detector |
| **Complete** | 2.6 Marketplace Navigation |
| **Complete** | 2.7 Heartbeat Integration |
| **Complete** | 2.8 Desktop UI |
| **Complete** | 2.10 Health Monitoring |
| **Complete** | 2.9 Dashboard contract (implemented in `mlt` repo) |
| **Complete** | 2.11 Testing |
| **Complete** | 2.12 Security and Privacy Review |
| **Complete** | 2.13 Packaging Preparation |

## Architecture

**Playwright runs in a Node sidecar** (`browser-sidecar/cli.mjs`) spawned by Rust. Clear process boundary; production will bundle Node + Playwright + Chromium (2.13).

```
Tauri (Rust)  ←JSON stdout→  browser-sidecar (Node + Playwright)  →  Chromium
```

Persistent profile, Facebook detection, and BrowserManager arrive in 2.2–2.6.

## Module layout

```
browser-sidecar/cli.mjs          # Node CLI (detect, launch-test, close-test)
src-tauri/src/browser/
  mod.rs                         # init + MLT_BROWSER_ENABLED
  types.rs                       # runtime states + snapshots
  sidecar.rs                     # spawn sidecar commands
  runtime.rs                     # BrowserRuntimeService (2.1)
  manager.rs                     # BrowserManager (2.2)
  profile.rs                     # persistent profile (2.3)
  facebook.rs                    # session detector (2.4–2.5)
  marketplace.rs                 # navigation (2.6)
src-tauri/src/services/
  browser_health.rs              # monitoring (2.10)
```

## Dependencies

| Package | Purpose |
|---|---|
| `playwright` (npm) | Browser automation engine |
| Node (dev: system; prod: bundled sidecar) | Runs Playwright |

## `mlt` contract changes (2.7+)

- Heartbeat: `facebook_session_state` (snake_case), `browser_status`, `marketplace_status`, etc.
- Test Connection: new checks for browser/runtime/Facebook/Marketplace
- Coordinate with Lovable; backend-first in `deviceMapper.ts`

## Commit sequence

1. `feat(m2.1): playwright runtime foundation`
2. `feat(m2.2): browser manager`
3. … through `docs(m2.13): packaging preparation for playwright sidecar` ← **Milestone 2 complete**

## Testing

Run the full Milestone 2 test suite:

```bash
npm test
```

(Rust unit/integration tests + sidecar Node tests — see 2.11.)

## Security and packaging

- [MILESTONE-2-SECURITY-MODEL.md](./MILESTONE-2-SECURITY-MODEL.md)
- [MILESTONE-2-PACKAGING.md](./MILESTONE-2-PACKAGING.md)

## Risks

| Risk | Mitigation |
|---|---|
| Large Chromium download | Explicit `npm run browser:install`; never auto-download on startup |
| Node not in production | Sidecar bundling plan (2.13); dev uses system Node |
| Profile lock multi-instance | Single-instance plugin + profile lock file (2.3) |
| Facebook DOM churn | Multi-signal detector, not one selector (2.5) |
