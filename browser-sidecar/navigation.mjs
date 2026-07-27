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
 * @returns {Promise<{ ok: true, attempt: number, current_url: string }>}
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
      return { ok: true, attempt, current_url: page.url() };
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
