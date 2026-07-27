# Milestone 2 — Packaging Preparation (Playwright Sidecar)

This document prepares Milestone 3+ packaging work. **No public installer is built in Milestone 2.**

Goal: shipped connector runs without Cursor, a dev terminal, global Node, global Playwright, or the project source folder.

---

## Runtime architecture (packaged)

```
MLT Desktop Connector.app (Tauri)
├── mlt-desktop-connector (Rust binary)
├── resources/
│   ├── browser-sidecar/          # Node entry + JS modules
│   │   ├── server.mjs
│   │   ├── cli.mjs
│   │   ├── facebook-detector.mjs
│   │   └── marketplace-evaluator.mjs
│   ├── node/                     # Platform-specific Node binary (LTS)
│   └── playwright/               # playwright package + browser bundle metadata
└── WebView UI (built assets)
```

Rust spawns `{resource_dir}/node/node` with `{resource_dir}/browser-sidecar/server.mjs` (same contract as dev, paths resolved at runtime).

---

## Playwright / Chromium bundling

### Playwright npm package

- Ship the `playwright` package (matching lockfile version, currently **1.52.x**) inside app resources.
- Set `PLAYWRIGHT_BROWSERS_PATH` (or equivalent) to a directory under app resources, e.g. `{resource_dir}/chromium/`.
- Run `playwright install chromium` **at build time**, not on end-user first launch.

### Chromium binary

- Bundle Playwright-managed Chromium for each target OS/arch (macOS universal or separate arm64/x64; Windows x64).
- Rust/sidecar `detect` reads the bundled path — no download on startup (aligns with M2 risk mitigation: never auto-download on startup).

### Node runtime

- Bundle **Node LTS** (20.x or 22.x) per platform in `resources/node/`.
- Dev continues to use system `node`; production resolves bundled binary via Tauri `resource_dir()` in `browser/sidecar.rs` (implementation in M3).

---

## Expected installer size (estimates)

| Component | Approximate size |
|---|---|
| Tauri app + UI | 15–25 MB |
| Node runtime | 40–55 MB |
| Playwright driver JS | 5–10 MB |
| Chromium (Playwright build) | 130–180 MB |
| **Total installer** | **~200–270 MB** |

Compressed DMG/MSI may be smaller; plan CDN/hosting accordingly.

---

## Chromium update strategy

1. **Pin** Playwright + Chromium revision in connector release (same minor Playwright as CI build).
2. **Connector updates** ship new Chromium when Playwright minor bumps require it — not silent mid-version updates.
3. **Security patches:** patch connector releases on cadence; rebuild with `playwright install chromium` in CI.
4. **Rollback:** keep previous installer artifact; user can reinstall prior `.app` / `.msi` (see Rollback below).
5. **No runtime download:** if bundled Chromium missing/corrupt, show actionable error — do not fetch from internet automatically.

---

## macOS code-signing implications

- Sign **Tauri app bundle**, bundled **Node**, and **Chromium** nested binaries (`Chromium.app/Contents/MacOS/Chromium`, helpers, `.framework` libs).
- Enable **hardened runtime** + appropriate entitlements:
  - Network client (API + Facebook)
  - User-selected / app-container file access for app data + diagnostics
- **Notarization** required for Gatekeeper distribution outside Mac App Store.
- Chromium spawn: child processes inherit signed parent; verify with `codesign --verify --deep`.
- Profile path: `~/Library/Application Support/com.7socialapps.mlt-desktop-connector/` (exact bundle ID TBD at packaging).

---

## Windows packaging implications

- **MSI or NSIS** via Tauri bundler; sign with Authenticode cert.
- Chromium + Node paths under `%LOCALAPPDATA%\Programs\mlt-desktop-connector\` or install dir `resources/`.
- App data: `%APPDATA%\com.7socialapps.mlt-desktop-connector\` (Tauri `app_data_dir`).
- Windows Defender SmartScreen: signed installer required for smooth install.
- Profile lock uses `tasklist` PID check (already implemented in Rust + sidecar).

---

## App-data paths by OS

| Purpose | macOS | Windows | Linux |
|---|---|---|---|
| Connector credentials | `~/Library/Application Support/{identifier}/credentials.enc` | `%APPDATA%/{identifier}/` | `~/.local/share/{identifier}/` |
| Browser profile | `…/browser-profile/` | `…/browser-profile/` | `…/browser-profile/` |
| Diagnostics PNGs | `…/diagnostics/` | `…/diagnostics/` | `…/diagnostics/` |
| Logs | `~/Library/Logs/{identifier}/` | `%LOCALAPPDATA%/{identifier}/logs/` | `~/.local/share/{identifier}/logs/` |

`{identifier}` = Tauri `identifier` from `tauri.conf.json` (set at packaging time).

---

## Browser runtime version compatibility

- **Pin** Playwright npm version in connector release metadata (`connector_version` + internal `PLAYWRIGHT_VERSION`).
- Sidecar `detect` reports `playwright_version` in heartbeat for backend visibility.
- MLT dashboard can warn if connector Playwright major differs from expected (future).
- Chromium must match Playwright's expected revision for that package version — do not mix arbitrary Chrome installs.

---

## Rollback strategy

| Layer | Rollback action |
|---|---|
| Connector app | Reinstall previous signed `.app` / `.msi` |
| Bundled Chromium | Restored with prior app version (co-located in resources) |
| Browser profile | User may reset profile from UI; does not affect device credentials |
| Device credentials | Persist across app downgrade unless explicitly revoked from dashboard |
| Backend contract | Heartbeat snake_case fields backward-compatible within M2 contract version |

Document release notes when Playwright bumps require profile re-login or profile reset.

---

## Dependencies the packaged app must NOT require

| Dependency | Dev today | Packaged target |
|---|---|---|
| Cursor IDE | Optional | **Not required** |
| Dev terminal / shell | Optional | **Not required** |
| Global `node` on PATH | Used in dev | **Bundled Node** |
| Global `playwright` CLI | Used in dev | **Bundled package** |
| Project source folder | Required in dev | **Not required** — resources embedded |
| User-installed Chrome | Not used | **Not used** — Playwright Chromium only |

---

## Build pipeline outline (M3+)

1. `npm ci` + `playwright install chromium` in CI with fixed `PLAYWRIGHT_BROWSERS_PATH` output dir.
2. Copy sidecar JS + Node + playwright + chromium into `src-tauri/resources/`.
3. `tauri build` per target (macOS universal, Windows x64).
4. Sign + notarize / Authenticode.
5. Smoke test packaged app: detect → launch → Facebook login → heartbeat (no repo checkout on runner beyond CI workspace).

---

## Related documents

- [MILESTONE-2-PLAYWRIGHT-BROWSER-FOUNDATION.md](./MILESTONE-2-PLAYWRIGHT-BROWSER-FOUNDATION.md) — implementation plan
- [MILESTONE-2-SECURITY-MODEL.md](./MILESTONE-2-SECURITY-MODEL.md) — privacy boundaries for packaged runtime
