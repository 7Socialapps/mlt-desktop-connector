/**
 * Pure Marketplace state evaluation from Facebook detection + URL (M2.11 tests).
 */
import { isMarketplaceUrl } from "./facebook-detector.mjs";

/**
 * @param {{ state: string, reason_code: string }} fb
 * @param {string} url
 * @returns {{ status: string, reason_code: string, current_url: string, facebook_state: string }}
 */
export function evaluateMarketplaceFromDetection(fb, url) {
  if (fb.state === "facebook_logged_out" || fb.state === "facebook_session_expired") {
    return {
      status: "marketplace_login_required",
      reason_code: fb.reason_code,
      current_url: url,
      facebook_state: fb.state,
    };
  }
  if (fb.state === "facebook_checkpoint") {
    return {
      status: "marketplace_checkpoint",
      reason_code: "facebook_checkpoint",
      current_url: url,
      facebook_state: fb.state,
    };
  }
  if (fb.state === "facebook_mfa_required" || fb.state === "facebook_login_in_progress") {
    return {
      status: "marketplace_login_required",
      reason_code: fb.reason_code,
      current_url: url,
      facebook_state: fb.state,
    };
  }
  if (isMarketplaceUrl(url)) {
    return {
      status: "marketplace_ready",
      reason_code: "marketplace_loaded",
      current_url: url,
      facebook_state: fb.state,
    };
  }
  if (fb.state === "facebook_logged_in" && !isMarketplaceUrl(url)) {
    return {
      status: "marketplace_unavailable",
      reason_code: "not_marketplace_url",
      current_url: url,
      facebook_state: fb.state,
    };
  }
  return {
    status: "marketplace_error",
    reason_code: "ambiguous_marketplace_state",
    current_url: url,
    facebook_state: fb.state,
  };
}
