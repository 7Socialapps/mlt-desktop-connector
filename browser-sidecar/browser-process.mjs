/**
 * Safe browser process helpers for Playwright persistent contexts.
 *
 * launchPersistentContext() returns a BrowserContext directly. The linked
 * Browser (from context.browser()) may not expose process() — unlike
 * chromium.launch() which always provides Browser.process().
 */

/**
 * Resolve Chromium PID when Playwright exposes it. Never throws.
 * @param {import("playwright").BrowserContext | null | undefined} ctx
 * @returns {number | null}
 */
export function resolveBrowserProcessPid(ctx) {
  if (!ctx) {
    return null;
  }
  try {
    const browser = ctx.browser();
    if (!browser) {
      return null;
    }
    if (typeof browser.process !== "function") {
      return null;
    }
    const proc = browser.process();
    return typeof proc?.pid === "number" ? proc.pid : null;
  } catch {
    return null;
  }
}

/**
 * Whether the managed browser context is still usable.
 * Does not require process() or browser().
 * @param {import("playwright").BrowserContext | null | undefined} ctx
 * @param {string} browserState
 * @returns {boolean}
 */
export function isBrowserContextConnected(ctx, browserState) {
  if (!ctx || browserState !== "ready") {
    return false;
  }
  try {
    const browser = ctx.browser();
    if (browser && typeof browser.isConnected === "function") {
      return browser.isConnected();
    }
    // Persistent context: browser() may be null — context.pages() works while open.
    ctx.pages();
    return true;
  } catch {
    return false;
  }
}

/**
 * Profile disk state when browser is actively running.
 * @param {string} browserState
 * @param {import("playwright").BrowserContext | null | undefined} ctx
 * @returns {string | null} profile state override, or null to inspect disk
 */
export function profileStateWhileBrowserRunning(browserState, ctx) {
  if (browserState === "ready" && ctx) {
    return "profile_ready";
  }
  if (browserState === "starting" && ctx) {
    return "profile_initializing";
  }
  return null;
}
