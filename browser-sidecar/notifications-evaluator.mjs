/**
 * Pure Notifications state evaluation from Facebook detection + URL (M4 framework).
 */
import { isFacebookUrl } from "./facebook-detector.mjs";

/**
 * @param {{ state: string, reason_code: string }} fb
 * @param {string} url
 * @returns {{ status: string, reason_code: string, current_url: string, facebook_state: string, unread_count: number | null }}
 */
export function evaluateNotificationsFromDetection(fb, url) {
  if (!url || url === "about:blank" || !isFacebookUrl(url)) {
    return {
      status: "notifications_not_checked",
      reason_code: "not_facebook",
      current_url: url || "",
      facebook_state: fb?.state ?? "facebook_not_checked",
      unread_count: null,
    };
  }

  if (fb.state === "facebook_logged_out" || fb.state === "facebook_session_expired") {
    return {
      status: "notifications_login_required",
      reason_code: fb.reason_code,
      current_url: url,
      facebook_state: fb.state,
      unread_count: null,
    };
  }

  if (
    fb.state === "facebook_checkpoint" ||
    fb.state === "facebook_mfa_required" ||
    fb.state === "facebook_login_in_progress" ||
    fb.state === "facebook_temporary_restriction" ||
    fb.state === "facebook_disabled_account"
  ) {
    return {
      status: "notifications_unavailable",
      reason_code: fb.reason_code,
      current_url: url,
      facebook_state: fb.state,
      unread_count: null,
    };
  }

  const lowerUrl = url.toLowerCase();
  if (lowerUrl.includes("/notifications")) {
    if (fb.state === "facebook_logged_in") {
      return {
        status: "notifications_ready",
        reason_code: "notifications_loaded",
        current_url: url,
        facebook_state: fb.state,
        unread_count: null,
      };
    }
  }

  if (fb.state === "facebook_logged_in") {
    return {
      status: "notifications_unavailable",
      reason_code: "not_notifications_url",
      current_url: url,
      facebook_state: fb.state,
      unread_count: null,
    };
  }

  return {
    status: "notifications_error",
    reason_code: "ambiguous_notifications_state",
    current_url: url,
    facebook_state: fb.state,
    unread_count: null,
  };
}
