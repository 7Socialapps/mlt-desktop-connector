# M4 Dashboard Contract — Facebook Runtime Status

Additive fields sent in the desktop connector **heartbeat** (`action: "heartbeat"`). All new fields are optional (`skip_serializing_if` none) for backward compatibility with dashboard builds that predate M4.

## Field reference

| Field | Type | Description |
|---|---|---|
| `browser_state` | string | Chromium lifecycle (`browser_ready`, `browser_stopped`, …) |
| `facebook_session_state` | string | Canonical session (`logged_in`, `logged_out`, `checkpoint`, `mfa_required`, `account_restricted`, `account_disabled`, `session_expired`, `unknown`) |
| `facebook_account_label` | string \| null | Display name from DOM when logged in — **never** email/phone |
| `marketplace_state` | string | Marketplace service state (`marketplace_ready`, …) |
| `messenger_state` | string | Messenger framework state (`messenger_ready`, …) |
| `notifications_state` | string | Notifications framework state (`notifications_ready`, …) |
| `current_destination` | string \| null | Last navigation destination key or URL category |
| `current_service` | string \| null | Active runtime service (`marketplace`, `messenger`, …) |
| `browser_pid` | number \| null | Managed Chromium PID |
| `browser_version` | string \| null | Playwright/Chromium version |
| `connector_version` | string | Desktop connector semver |
| `profile_version` | string | Opaque hash of profile path — **not** filesystem path |
| `last_health_check_at` | ISO8601 \| null | Last browser health poll |
| `last_restart_at` | ISO8601 \| null | Last browser restart |
| `last_navigation_error` | string \| null | Sanitized navigation failure (no tokens) |

## Legacy fields (retained)

M2/M3 fields remain unchanged: `browser_status`, `facebook_session_state` (legacy enum), `marketplace_status`, `marketplace_ready`, `messenger_ready`, `notifications_ready`, `current_facebook_account`, etc.

## Backend normalization (mlt — not in this repo)

When implementing dashboard consumption, map canonical `facebook_session_state` values in `browserStatus.ts`. Do not merge unrelated Lovable work when adding normalization.

## Security

Never include in heartbeat: cookies, refresh tokens, access tokens, credential paths, full Facebook URLs, diagnostic screenshot paths, or customer PII beyond optional display name.
