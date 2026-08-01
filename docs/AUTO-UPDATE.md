# Auto-update foundation (not yet enabled)

**Status:** Documentation only — no updater plugin wired in v1.0.0.

## Goals

- Ship connector fixes without manual DMG redistribution.
- Respect staging vs production channels.
- Never auto-update across environments (staging build must not pull production artifacts).

## Planned approach (Tauri 2)

1. Add `tauri-plugin-updater` when signing/notarization is available (required for trustworthy updates on macOS).
2. Host update manifests per channel:
   - `https://<cdn>/mlt-desktop-connector/staging/latest.json`
   - `https://cdn>/mlt-desktop-connector/production/latest.json`
3. Embed channel at build time via `MLT_ENV` (already compiled through `build.rs` / `config.rs`).
4. Updater checks manifest signature (minisign or Apple-notarized bundle hash — TBD with security review).
5. Download in background; prompt user before restart (tray notification + status window).

## Manifest sketch

```json
{
  "version": "1.0.1",
  "notes": "Facebook session stability fixes",
  "pub_date": "2026-07-27T12:00:00Z",
  "platforms": {
    "darwin-aarch64": {
      "signature": "...",
      "url": "https://cdn.example/mlt-desktop-connector/staging/1.0.1/MLT_Desktop_Connector_1.0.1_aarch64.dmg"
    }
  }
}
```

## Configuration placeholders

When enabled, add to `tauri.conf.json`:

```json
"plugins": {
  "updater": {
    "active": false,
    "endpoints": [],
    "dialog": true
  }
}
```

## Rollout checklist

- [ ] Apple Developer ID signing + notarization (`docs/SIGNING-STATUS.md`)
- [ ] CDN bucket with versioned DMGs
- [ ] CI job publishing manifest after tagged release
- [ ] Staging soak test before promoting manifest to production channel
- [ ] Rollback procedure: publish previous manifest version

## Rollback

Disable updater in config (`active: false`) and redeploy last known-good DMG link to dashboard download page.
