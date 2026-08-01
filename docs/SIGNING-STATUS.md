# macOS signing status

**Last audited:** 2026-07-27

## Current status

| Item | Status |
|---|---|
| Apple Developer ID certificate | **Not configured** |
| `signingIdentity` in `tauri.conf.json` | `null` (placeholder) |
| Entitlements file | **Not configured** (`null`) |
| Notarization / stapling | **Not configured** |
| CI signing secrets | **Not configured** |

Builds from `npm run build:staging` produce **unsigned** `.app` and `.dmg` bundles.

## Installing unsigned staging builds (macOS)

1. Build: `npm run build:staging` (requires `.env` with staging Supabase anon key — never commit).
2. Open the `.dmg` from:
   `src-tauri/target/release/bundle/dmg/`
3. Drag **MLT Desktop Connector** to Applications.
4. First launch — macOS Gatekeeper may block the app:
   - Open **System Settings → Privacy & Security**.
   - Click **Open Anyway** for MLT Desktop Connector, or right-click the app → **Open**.
5. Register protocol handler by launching the app once; links `mlt-desktop://…` route to the running connector (single-instance).

## Future signing setup (placeholder)

When Apple Developer ID is available:

1. Import certificate to login keychain.
2. Set in `src-tauri/tauri.conf.json`:
   ```json
   "macOS": {
     "signingIdentity": "Developer ID Application: Your Org (TEAMID)",
     "entitlements": "entitlements.plist"
   }
   ```
3. Add notarization env vars to CI (`APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID`).
4. Enable `tauri build` notarization flags in release pipeline.

## Rollback

Revert to prior unsigned `.dmg` from git tag or rebuild from previous commit; no notarization ticket revocation required while unsigned.
