/**
 * Pure Messenger state evaluation from Facebook detection + URL (M4 framework).
 */
import { isFacebookUrl } from "./facebook-detector.mjs";

/**
 * @param {{ state: string, reason_code: string }} fb
 * @param {string} url
 * @returns {{ status: string, reason_code: string, current_url: string, facebook_state: string }}
 */
export function evaluateMessengerFromDetection(fb, url) {
  if (!url || url === "about:blank" || !isFacebookUrl(url)) {
    return {
      status: "messenger_not_checked",
      reason_code: "not_facebook",
      current_url: url || "",
      facebook_state: fb?.state ?? "facebook_not_checked",
    };
  }

  if (fb.state === "facebook_logged_out" || fb.state === "facebook_session_expired") {
    return {
      status: "messenger_login_required",
      reason_code: fb.reason_code,
      current_url: url,
      facebook_state: fb.state,
    };
  }

  if (fb.state === "facebook_checkpoint") {
    return {
      status: "messenger_checkpoint",
      reason_code: "facebook_checkpoint",
      current_url: url,
      facebook_state: fb.state,
    };
  }

  if (
    fb.state === "facebook_mfa_required" ||
    fb.state === "facebook_login_in_progress" ||
    fb.state === "facebook_temporary_restriction" ||
    fb.state === "facebook_disabled_account"
  ) {
    return {
      status: "messenger_unavailable",
      reason_code: fb.reason_code,
      current_url: url,
      facebook_state: fb.state,
    };
  }

  const lowerUrl = url.toLowerCase();
  if (lowerUrl.includes("/messages") || lowerUrl.includes("/messenger")) {
    if (fb.state === "facebook_logged_in") {
      return {
        status: "messenger_ready",
        reason_code: "messenger_loaded",
        current_url: url,
        facebook_state: fb.state,
      };
    }
  }

  if (fb.state === "facebook_logged_in") {
    return {
      status: "messenger_unavailable",
      reason_code: "not_messenger_url",
      current_url: url,
      facebook_state: fb.state,
    };
  }

  return {
    status: "messenger_error",
    reason_code: "ambiguous_messenger_state",
    current_url: url,
    facebook_state: fb.state,
  };
}
