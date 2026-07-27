/**
 * Deterministic Facebook navigation destinations (M4).
 */

/** @typedef {"facebook_home"|"marketplace"|"marketplace_create_vehicle"|"messenger"|"notifications"} NavigationDestination */

export const DESTINATIONS = {
  facebook_home: "https://www.facebook.com/",
  marketplace: "https://www.facebook.com/marketplace/",
  marketplace_create_vehicle: "https://www.facebook.com/marketplace/create/vehicle",
  messenger: "https://www.facebook.com/messages/",
  notifications: "https://www.facebook.com/notifications",
};

/**
 * Detect unexpected redirects (login, checkpoint, wrong section).
 * @param {string} targetUrl
 * @param {string} currentUrl
 */
export function detectRedirect(targetUrl, currentUrl) {
  if (!targetUrl || !currentUrl) return false;
  try {
    const target = new URL(targetUrl);
    const current = new URL(currentUrl);
    if (current.pathname.includes("/login") || current.pathname.includes("/checkpoint")) {
      return true;
    }
    if (target.pathname.startsWith("/marketplace") && !current.pathname.startsWith("/marketplace")) {
      return true;
    }
    if (target.pathname.startsWith("/messages") && !current.pathname.startsWith("/messages")) {
      return true;
    }
    if (
      target.pathname.startsWith("/notifications") &&
      !current.pathname.startsWith("/notifications")
    ) {
      return true;
    }
    return false;
  } catch {
    return false;
  }
}

/**
 * @param {NavigationDestination | string} destination
 * @returns {string | null}
 */
export function destinationUrl(destination) {
  return DESTINATIONS[destination] ?? null;
}


/**
 * @param {import("playwright").Page} page
 * @param {string} targetUrl
 * @param {number} [maxAttempts]
 * @param {number} [timeoutMs]
 * @returns {Promise<{ ok: true, attempt: number, current_url: string, redirect_detected: boolean }>}
 */
export async function navigateWithRetry(page, targetUrl, maxAttempts = 2, timeoutMs = 45_000) {
  let lastError = null;
  for (let attempt = 1; attempt <= maxAttempts; attempt++) {
    try {
      await page.goto(targetUrl, {
        waitUntil: "domcontentloaded",
        timeout: timeoutMs,
      });
      await page.waitForTimeout(2000);
      const current_url = page.url();
      return {
        ok: true,
        attempt,
        current_url,
        redirect_detected: detectRedirect(targetUrl, current_url),
      };
    } catch (err) {
      lastError = err;
      if (attempt < maxAttempts) {
        await page.waitForTimeout(1000 * attempt);
      }
    }
  }
  throw lastError;
}

/**
 * Lightweight page readiness after navigation.
 * @param {import("playwright").Page} page
 */
export async function waitForPageReady(page) {
  try {
    await page.waitForLoadState("domcontentloaded", { timeout: 10_000 });
  } catch {
    /* navigation may still be usable */
  }
  await page.waitForTimeout(500);
  return { url: page.url(), title: await page.title().catch(() => "") };
}
