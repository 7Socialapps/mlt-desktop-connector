/**
 * Prefer system Google Chrome / Edge for Facebook login (Touch ID, passkeys, familiar UX).
 * Fall back to bundled Playwright Chromium ("Chrome for Testing") when none are installed.
 */
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

/**
 * @typedef {{
 *   mode: "system_chrome" | "system_edge" | "bundled_chromium",
 *   channel: "chrome" | "msedge" | null,
 *   label: string,
 *   process_name_hint: string,
 *   executable_path: string | null,
 * }} BrowserLaunchTarget
 */

/** @returns {string[]} */
function chromeCandidatePaths() {
  if (process.platform === "darwin") {
    return [
      "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
      path.join(
        os.homedir(),
        "Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
      ),
    ];
  }
  if (process.platform === "win32") {
    const local = process.env.LOCALAPPDATA || "";
    const pf = process.env.PROGRAMFILES || "C:\\Program Files";
    const pf86 = process.env["PROGRAMFILES(X86)"] || "C:\\Program Files (x86)";
    return [
      path.join(pf, "Google", "Chrome", "Application", "chrome.exe"),
      path.join(pf86, "Google", "Chrome", "Application", "chrome.exe"),
      path.join(local, "Google", "Chrome", "Application", "chrome.exe"),
    ];
  }
  return [
    "/usr/bin/google-chrome-stable",
    "/usr/bin/google-chrome",
    "/usr/bin/chromium-browser",
    "/usr/bin/chromium",
  ];
}

/** @returns {string[]} */
function edgeCandidatePaths() {
  if (process.platform === "darwin") {
    return [
      "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
      path.join(
        os.homedir(),
        "Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
      ),
    ];
  }
  if (process.platform === "win32") {
    const pf = process.env.PROGRAMFILES || "C:\\Program Files";
    const pf86 = process.env["PROGRAMFILES(X86)"] || "C:\\Program Files (x86)";
    return [
      path.join(pf, "Microsoft", "Edge", "Application", "msedge.exe"),
      path.join(pf86, "Microsoft", "Edge", "Application", "msedge.exe"),
    ];
  }
  return ["/usr/bin/microsoft-edge", "/usr/bin/microsoft-edge-stable"];
}

/** @param {string[]} candidates */
function firstExisting(candidates) {
  for (const p of candidates) {
    if (p && fs.existsSync(p)) return p;
  }
  return null;
}

/**
 * Resolve preferred headed browser for Facebook login.
 * Set MLT_FORCE_BUNDLED_BROWSER=1 to skip system Chrome/Edge (tests / diagnostics).
 * @returns {BrowserLaunchTarget}
 */
export function resolveBrowserLaunchTarget() {
  const forceBundled =
    process.env.MLT_FORCE_BUNDLED_BROWSER === "1" ||
    process.env.MLT_FORCE_BUNDLED_BROWSER === "true";

  if (!forceBundled) {
    const chromePath = firstExisting(chromeCandidatePaths());
    if (chromePath) {
      return {
        mode: "system_chrome",
        channel: "chrome",
        label: "Google Chrome",
        process_name_hint: "Google Chrome",
        executable_path: chromePath,
      };
    }
    const edgePath = firstExisting(edgeCandidatePaths());
    if (edgePath) {
      return {
        mode: "system_edge",
        channel: "msedge",
        label: "Microsoft Edge",
        process_name_hint: "Microsoft Edge",
        executable_path: edgePath,
      };
    }
  }

  return {
    mode: "bundled_chromium",
    channel: null,
    label: "bundled browser (install Google Chrome for easiest login)",
    process_name_hint: "Chrome for Testing",
    executable_path: null,
  };
}

/** Dealer-facing URL when system Chrome is missing. */
export function chromeInstallUrl() {
  return "https://www.google.com/chrome/";
}

/** Copy shown when we had to fall back to bundled Chromium. */
export function bundledBrowserDealerMessage() {
  return `Install Google Chrome for easiest Facebook login: ${chromeInstallUrl()} — then click Open Facebook again. The built-in browser often blocks passkeys.`;
}

/**
 * Shared launch args for headed Facebook login.
 * Reduces obvious automation chrome where practical (Meta may still challenge).
 */
export function browserLaunchArgs() {
  return [
    "--disable-dev-shm-usage",
    "--disable-blink-features=AutomationControlled",
    "--new-window",
    "--window-size=1280,900",
    "--window-position=80,60",
    "--start-maximized",
  ];
}

/**
 * Playwright defaults that break normal Chrome login UX:
 * - --enable-automation → "Chrome is being controlled…" + automation signals
 * - --use-mock-keychain / --password-store=basic → block real Keychain / Touch ID / passkeys
 */
export function browserIgnoreDefaultArgs() {
  return [
    "--enable-automation",
    "--use-mock-keychain",
    "--password-store=basic",
  ];
}
