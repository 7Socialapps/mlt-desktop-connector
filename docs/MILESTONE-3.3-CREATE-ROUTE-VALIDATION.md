# Milestone 3.3 — Create Route Validation

Milestone M3.3 validates the Facebook Marketplace **vehicle create route** for `prepare_for_review` jobs. It navigates through the shared `FacebookRuntime`, verifies the create form is ready, and stops at `create_route_ready` (progress 100%). It does **not** fill fields, upload images, click Publish, or call `complete_job`.

## Scope

### In scope

- Job executor orchestrating: asset download → runtime start → session check → Marketplace → vehicle create route → form verification
- Structured job phases, error codes, and progress updates with runtime context
- Sidecar `verify_vehicle_create` RPC with multi-signal readiness checks
- `MarketplaceService.open_vehicle_create_route()` and `verify_vehicle_create_form()`
- Polling integration with idempotency (active job tracking) and cancellation (`ServiceBus.is_cancelled()`)
- UI commands mirroring production runtime services
- Local job evidence screenshots (no secrets)

### Out of scope (later milestones)

- Field filling, Facebook image upload, Next/Publish clicks
- Messenger scraping, AI replies
- `complete_job` / published state

## Job phases

| Phase | Status | Progress | Description |
|---|---|---|---|
| Queued | `queued` | 0 | Job received from poll |
| Claimed | `claimed` | 5 | Job claimed with scoped token |
| Validating payload | `validating_payload` | 10 | Payload contract validation |
| Preparing assets | `preparing_assets` | 30 | Download listing photos |
| Starting runtime | `starting_runtime` | 40 | Ensure browser ready |
| Checking Facebook session | `checking_facebook_session` | 50 | Session precheck |
| Opening Marketplace | `opening_marketplace` | 60 | Navigate to Marketplace home |
| Opening vehicle create | `opening_vehicle_create` | 75 | Navigate to create/vehicle |
| Verifying vehicle create | `verifying_vehicle_create` | 85 | Sidecar form verification |
| **Terminal success** | `create_route_ready` | **100** | Route validated — ready for dealer review |
| Cancelled | `cancelled` | 0 | User cancelled via UI |
| Failed | `failed` | 0 | Structured error via `fail_job` |

## Error codes

| Code | When |
|---|---|
| `PAYLOAD_VALIDATION_FAILED` | Payload contract invalid |
| `IMAGE_DOWNLOAD_FAILED` | Listing photo download/validation failed |
| `BROWSER_NOT_READY` | Browser/sidecar not operational |
| `FACEBOOK_LOGGED_OUT` | Session not signed in |
| `FACEBOOK_CHECKPOINT` | Security checkpoint |
| `FACEBOOK_MFA_REQUIRED` | MFA required |
| `FACEBOOK_SESSION_EXPIRED` | Session expired |
| `FACEBOOK_ACCOUNT_RESTRICTED` | Temporary restriction |
| `FACEBOOK_ACCOUNT_DISABLED` | Account disabled |
| `FACEBOOK_SESSION_UNKNOWN` | Session state indeterminate |
| `MARKETPLACE_NAV_FAILED` | Marketplace navigation error |
| `MARKETPLACE_NOT_READY` | Marketplace not in ready state |
| `VEHICLE_CREATE_ROUTE_NOT_READY` | Create route navigation failed |
| `VEHICLE_CREATE_VERIFICATION_FAILED` | Form verification signals missing |
| `OPERATION_CANCELLED` | User cancelled current operation |
| `RUNTIME_ERROR` | Unexpected runtime failure |

## Architecture

```
PollingService
  └── PrepareForReviewExecutor (marketplace/jobs/executor.rs)
        ├── download_job_assets (M3 assets)
        └── FacebookRuntime ONLY (no direct sidecar from jobs)
              ├── ServiceBus (mutex, cancellation)
              ├── FacebookSessionService.check_session()
              ├── MarketplaceService.open_marketplace()
              ├── MarketplaceService.open_vehicle_create_route()
              │     ├── NavigationService.navigate_with_recovery()
              │     └── verify_vehicle_create (sidecar RPC)
              └── DiagnosticsService (status context)
```

## Sidecar verification signals

`vehicle-create-verifier.mjs` evaluates:

1. URL on `/marketplace/create/vehicle`
2. Create heading (vehicle / create listing / sell)
3. Form landmarks (form, labels, aria)
4. Photo upload area (file input or upload text)
5. Vehicle controls (year/make/model selectors or text)
6. No login form or checkpoint text

RPC: `verify_vehicle_create` → `{ vehicle_create: { ready, reason_code, signals_met, signals_missing, ... } }`

## UI commands (production runtime)

| Command | Service |
|---|---|
| `browser_launch` | BrowserManager |
| `browser_detect_facebook_session` | FacebookSessionService |
| `browser_open_marketplace` | MarketplaceService |
| `browser_open_vehicle_create` | MarketplaceService.open_vehicle_create_route |
| `runtime_cancel_operation` | ServiceBus.request_cancel |
| `browser_restart` | BrowserManager |
| `runtime_diagnostics_snapshot` | DiagnosticsService |
| `runtime_status` | FacebookRuntime.aggregate_status |

## Idempotency & cancellation

- **Idempotency**: `PollingService.active_job_id` prevents duplicate execution of the same job while in-flight.
- **Cancellation**: UI calls `runtime_cancel_operation`; executor checks `ServiceBus.is_cancelled()` between phases. Cancel flag cleared on job completion.

## Terminal success contract

Successful M3.3 jobs emit:

```json
{
  "status": "create_route_ready",
  "progress": 100,
  "current_step": "Vehicle create route validated — ready for dealer review"
}
```

Jobs do **not** call `complete_job`. The dashboard should show the job awaiting dealer review, not posted.

## Rollback

1. Revert feature branch commits.
2. Redeploy prior connector build.
3. No database migrations required.

## Test plan

- `npm test` — Node sidecar tests + Rust unit tests (121+)
- Manual: Launch browser → Check Facebook Session → Open Marketplace → Open Vehicle Create Form
- Manual: Submit `prepare_for_review` job; confirm terminal `create_route_ready` at 100%
- Manual: Cancel during job; confirm `OPERATION_CANCELLED`
- Confirm `transport_test` still completes via `complete_job`
