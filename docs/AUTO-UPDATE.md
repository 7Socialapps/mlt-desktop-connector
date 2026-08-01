# Auto-update (GitHub Releases)

**Status:** Enabled in **1.0.6+** (best-effort for unsigned builds).

## How it works

1. On launch (after ~8s) and every **4 hours**, the connector calls the public GitHub Releases API:
   `https://api.github.com/repos/7Socialapps/mlt-desktop-connector/releases/latest`
2. Compares the release tag (semver) to the embedded `CONNECTOR_VERSION`.
3. If newer, downloads the matching platform asset (never opens the GitHub HTML page):
   - macOS: `*aarch64.dmg` / `*arm64.dmg` or `*x64.dmg` / `*x86_64.dmg`
   - Windows: `*setup.exe` (preferred) or `*.msi`
4. Opens the installer locally:
   - **macOS:** mounts/opens the DMG
   - **Windows:** launches the setup EXE/MSI
5. UI switches from **Updating…** to **Installer open** with one clear line and:
   - **I’ve finished installing** — relaunches from `/Applications` (Mac) and exits the old process
   - **Open installer again** — reopens the downloaded DMG/EXE
   - After **2 minutes** still on the old binary → **Install stalled** with Retry / Open installer again (never leave Updating disabled forever).

Debug (`tauri dev`) builds skip automatic checks. Manual trigger still works via deep link.

## Why not `tauri-plugin-updater` yet

Official Tauri updater requires **signed updater artifacts** + a pubkey in `tauri.conf.json`. Builds are currently unsigned (`docs/SIGNING-STATUS.md`). Using the plugin without signing keys would refuse installs.

When Apple Developer ID + notarization (and optional Tauri minisign keys) are ready, migrate to `tauri-plugin-updater` for silent replace/relaunch.

## Deep link (optional dashboard)

```
mlt-desktop://check-update
```

Dashboard Connect may call this to hint “checking for updates” — not required for MVP; launch polling is enough.

## Signing caveats (macOS)

| Capability | Unsigned beta | Signed + notarized |
|---|---|---|
| Detect newer release | Yes | Yes |
| Download DMG | Yes | Yes |
| Silent replace + relaunch | No (Gatekeeper) | Yes (with Tauri updater) |
| User finishes install | Drag app to Applications | Optional prompt / auto |

## Rollback

- Mark previous GitHub Release as latest, or delete the bad release tag.
- Users already on a bad build: reinstall prior DMG/EXE from Releases API assets.
- Disable by shipping a build that no-ops the updater service (revert this feature).

## Release checklist

1. Bump `package.json`, `src-tauri/tauri.conf.json`, `Cargo.toml`, `version.rs`.
2. Build Mac aarch64 (+ x64) production DMGs; upload to `vX.Y.Z`.
3. Trigger Windows CI: `gh workflow run release-windows.yml -f release_tag=vX.Y.Z`
4. Confirm: `curl -s …/releases/latest | jq '.tag_name, .assets[].name'`
5. Install prior version → launch → confirm Updating… → finish install → About shows new version.
