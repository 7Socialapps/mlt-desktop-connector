/**
 * Facebook session detector — multi-signal, no credential logging (M2.5+).
 * @typedef {"facebook_not_checked"|"facebook_logged_out"|"facebook_login_in_progress"|"facebook_logged_in"|"facebook_checkpoint"|"facebook_mfa_required"|"facebook_session_expired"|"facebook_temporary_restriction"|"facebook_disabled_account"|"facebook_error"} FacebookState
 */

const FACEBOOK_HOSTS = new Set(["www.facebook.com", "facebook.com", "m.facebook.com"]);

/**
 * @param {string} url
 * @returns {boolean}
 */
export function isFacebookUrl(url) {
  try {
    const parsed = new URL(url);
    return FACEBOOK_HOSTS.has(parsed.hostname);
  } catch {
    return false;
  }
}

/**
 * @param {string} url
 * @returns {boolean}
 */
export function isMarketplaceUrl(url) {
  try {
    const parsed = new URL(url);
    return (
      FACEBOOK_HOSTS.has(parsed.hostname) &&
      parsed.pathname.startsWith("/marketplace")
    );
  } catch {
    return false;
  }
}

/**
 * Detect Facebook session state from page signals.
 * @param {{ url: string, title: string, hasLoginForm: boolean, hasCheckpointText: boolean, hasMfaText: boolean, hasNavBar: boolean, hasLogoutSignal: boolean, hasTemporaryRestrictionText?: boolean, hasDisabledAccountText?: boolean, displayName?: string | null }} signals
 * @returns {{ state: FacebookState, reason_code: string, marketplace_accessible: boolean }}
 */
export function detectFacebookSession(signals) {
  const { url, title, hasLoginForm, hasCheckpointText, hasMfaText, hasNavBar, hasLogoutSignal } =
    signals;

  if (!url || url === "about:blank") {
    return {
      state: "facebook_not_checked",
      reason_code: "no_page",
      marketplace_accessible: false,
    };
  }

  if (!isFacebookUrl(url)) {
    return {
      state: "facebook_not_checked",
      reason_code: "not_facebook",
      marketplace_accessible: false,
    };
  }

  const lowerTitle = (title || "").toLowerCase();
  const lowerUrl = url.toLowerCase();

  if (
    lowerUrl.includes("/checkpoint") ||
    lowerUrl.includes("/security_checkpoint") ||
    hasCheckpointText
  ) {
    return {
      state: "facebook_checkpoint",
      reason_code: "checkpoint_url",
      marketplace_accessible: false,
    };
  }

  if (
    lowerUrl.includes("/disabled") ||
    lowerTitle.includes("account disabled") ||
    lowerTitle.includes("account has been disabled") ||
    signals.hasDisabledAccountText
  ) {
    return {
      state: "facebook_disabled_account",
      reason_code: "account_disabled",
      marketplace_accessible: false,
    };
  }

  if (
    lowerUrl.includes("/restricted") ||
    lowerTitle.includes("temporarily blocked") ||
    lowerTitle.includes("temporary restriction") ||
    signals.hasTemporaryRestrictionText
  ) {
    return {
      state: "facebook_temporary_restriction",
      reason_code: "temporary_restriction",
      marketplace_accessible: false,
    };
  }

  if (
    lowerUrl.includes("/two_step_verification") ||
    lowerUrl.includes("/login/device-based") ||
    hasMfaText ||
    lowerTitle.includes("two-factor") ||
    lowerTitle.includes("authentication app")
  ) {
    return {
      state: "facebook_mfa_required",
      reason_code: "mfa_prompt",
      marketplace_accessible: false,
    };
  }

  if (hasLoginForm || lowerUrl.includes("/login")) {
    if (lowerUrl.includes("login_attempt")) {
      return {
        state: "facebook_login_in_progress",
        reason_code: "login_attempt",
        marketplace_accessible: false,
      };
    }
    return {
      state: "facebook_logged_out",
      reason_code: "login_page",
      marketplace_accessible: false,
    };
  }

  if (
    lowerUrl.includes("session_expired") ||
    lowerTitle.includes("session expired")
  ) {
    return {
      state: "facebook_session_expired",
      reason_code: "session_expired",
      marketplace_accessible: false,
    };
  }

  if (hasLogoutSignal || hasNavBar) {
    const marketplaceAccessible =
      isMarketplaceUrl(url) ||
      (!lowerUrl.includes("/login") && !hasLoginForm);
    return {
      state: "facebook_logged_in",
      reason_code: hasNavBar ? "nav_present" : "logout_signal",
      marketplace_accessible: marketplaceAccessible,
    };
  }

  if (isFacebookUrl(url) && !hasLoginForm) {
    return {
      state: "facebook_logged_in",
      reason_code: "facebook_url_no_login",
      marketplace_accessible: isMarketplaceUrl(url),
    };
  }

  return {
    state: "facebook_error",
    reason_code: "ambiguous_state",
    marketplace_accessible: false,
  };
}

/**
 * Collect DOM signals from a Playwright page (no sensitive data logged).
 * @param {import("playwright").Page} page
 */
export async function collectFacebookSignals(page) {
  let url = "about:blank";
  let title = "";
  try {
    url = page.url();
    title = await page.title();
  } catch {
    return {
      url: "about:blank",
      title: "",
      hasLoginForm: false,
      hasCheckpointText: false,
      hasMfaText: false,
      hasNavBar: false,
      hasLogoutSignal: false,
      hasTemporaryRestrictionText: false,
      hasDisabledAccountText: false,
      displayName: null,
    };
  }

  let domSignals = {
    hasLoginForm: false,
    hasCheckpointText: false,
    hasMfaText: false,
    hasNavBar: false,
    hasLogoutSignal: false,
    hasTemporaryRestrictionText: false,
    hasDisabledAccountText: false,
    displayName: null,
  };

  try {
    domSignals = await page.evaluate(() => {
      const bodyText = (document.body?.innerText ?? "").toLowerCase();
      const hasLoginForm = Boolean(
        document.querySelector('input[name="email"], input[name="pass"], form[data-testid="royal_login_form"]'),
      );
      const hasNavBar = Boolean(
        document.querySelector('[role="navigation"], [data-pagelet="LeftRail"]'),
      );
      const hasLogoutSignal = Boolean(
        document.querySelector('[aria-label="Account"], [aria-label="Your profile"]'),
      );
      const profileLink = document.querySelector('[aria-label="Your profile"], [aria-label="Account"]');
      const displayName =
        profileLink?.getAttribute("aria-label")?.replace(/^Your profile,?\s*/i, "").trim() ||
        profileLink?.textContent?.trim() ||
        null;
      return {
        hasLoginForm,
        hasCheckpointText:
          bodyText.includes("checkpoint") ||
          bodyText.includes("confirm your identity"),
        hasMfaText:
          bodyText.includes("two-factor") ||
          bodyText.includes("authentication code") ||
          bodyText.includes("login code"),
        hasNavBar,
        hasLogoutSignal,
        hasTemporaryRestrictionText:
          bodyText.includes("temporarily blocked") ||
          bodyText.includes("temporary restriction"),
        hasDisabledAccountText:
          bodyText.includes("account disabled") ||
          bodyText.includes("account has been disabled"),
        displayName: displayName || null,
      };
    });
  } catch {
    /* page may be navigating */
  }

  return { url, title, ...domSignals };
}

/**
 * Full detection from a Playwright page.
 * @param {import("playwright").Page} page
 */
export async function detectFromPage(page) {
  const signals = await collectFacebookSignals(page);
  const result = detectFacebookSession(signals);
  return {
    ...result,
    checked_at: new Date().toISOString(),
    current_url: signals.url,
    display_name: signals.displayName ?? null,
  };
}
