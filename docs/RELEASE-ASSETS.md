# Desktop Connector — GitHub Release assets

**Repo:** `7Socialapps/mlt-desktop-connector`  
**Dashboard consumer:** `mlt` → `src/lib/webposter/desktopConnectorDownloads.ts`

The dashboard resolves installers from the **latest GitHub Release** (not a CDN yet).
Until assets are attached, the UI opens this page and does **not** invent a file URL.

## Canonical URLs

| Purpose | URL |
|---|---|
| Releases list | `https://github.com/7Socialapps/mlt-desktop-connector/releases` |
| Latest release page | `https://github.com/7Socialapps/mlt-desktop-connector/releases/latest` |
| Latest release API | `https://api.github.com/repos/7Socialapps/mlt-desktop-connector/releases/latest` |

## Expected asset filenames (Tauri defaults)

Publish these names (version from `src-tauri/tauri.conf.json`, currently `1.0.4`):

| Platform | Example asset name | Notes |
|---|---|---|
| macOS Apple Silicon | `MLT Desktop Connector_1.0.4_aarch64.dmg` | Primary Mac download |
| macOS Intel | `MLT Desktop Connector_1.0.4_x64.dmg` | Optional |
| Windows | `MLT Desktop Connector_1.0.4_x64-setup.exe` or `…_x64_en-US.msi` | Primary Windows download |

Dashboard matchers (case-insensitive):

- Mac arm64: `aarch64.dmg`, `arm64.dmg`
- Mac x64: `x64.dmg`, `x86_64.dmg`
- Windows: `.msi`, `x64*.exe`, `setup.exe`, `.exe`

## Publish checklist (operator)

1. Build signed/notarized installers when Apple/Windows signing is ready (`docs/SIGNING-STATUS.md`).
2. Tag: `v1.0.4` (match app version).
3. `gh release create v1.0.4 ./path/to/*.dmg ./path/to/*.msi --title "MLT Desktop Connector 1.0.4" --notes "…"`.
4. Confirm API returns assets:  
   `curl -s https://api.github.com/repos/7Socialapps/mlt-desktop-connector/releases/latest | jq '.assets[].name'`.
5. Dashboard “Download & Install” buttons light up automatically — no dashboard redeploy required for new asset URLs.

## Current status (2026-08-01)

| Item | Status |
|---|---|
| GitHub Release `v1.0.3` | **Published** (allowlisted beta) |
| macOS Apple Silicon `.dmg` | **Published** — `…_aarch64.dmg` |
| macOS Intel `.dmg` | **Published** — `…_x64.dmg` |
| Windows `.exe` / `.msi` | **CI** — `.github/workflows/release-windows.yml` (workflow_dispatch) |
| Apple notarization | **Not configured** (`docs/SIGNING-STATUS.md`) |
| Windows code signing | **Not configured** |

Build Intel Mac from Apple Silicon: `rustup target add x86_64-apple-darwin` then  
`npm run tauri -- build --target x86_64-apple-darwin` (or package the `.app` with `hdiutil` if `bundle_dmg.sh` fails in CI/sandbox).

Do not upload an incomplete or unsigned build as a “production” latest release without labeling it **Pre-release** and warning allowlisted testers about Gatekeeper / SmartScreen.

## Rollback

- Delete or unpublish the bad release tag, or mark an older release as latest.
- Dashboard falls back to the releases page when no matching asset exists.
