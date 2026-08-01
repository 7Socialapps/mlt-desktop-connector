/**
 * Prefer system Google Chrome / Microsoft Edge for Facebook login
 * (Touch ID, passkeys, familiar UX). Fall back to bundled Playwright
 * Chromium ("Chrome for Testing") when neither is installed.
 *
 * Preference order: Chrome → Edge → bundled Chromium.
 * Persistent profiles are per engine so cookies never clash:
 *   chrome-profile / edge-profile / browser-profile (bundled + legacy).
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
 * Resolve preferred headed browser for Facebook login / posting.
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
    label: "bundled browser (install Google Chrome or Microsoft Edge for easiest login)",
    process_name_hint: "Chrome for Testing",
    executable_path: null,
  };
}

/** Dealer-facing URL when system Chrome is missing. */
export function chromeInstallUrl() {
  return "https://www.google.com/chrome/";
}

/** Dealer-facing URL when system Edge is an option. */
export function edgeInstallUrl() {
  return "https://www.microsoft.com/edge";
}

/**
 * Parent of the configured profile dir (`…/browser-profile` → app data).
 * @param {string} configuredProfileDir
 */
export function profileParentDir(configuredProfileDir) {
  if (!configuredProfileDir) return "";
  return path.dirname(configuredProfileDir);
}

/** @param {string} dir */
function hasProfileData(dir) {
  if (!dir || !fs.existsSync(dir)) return false;
  try {
    if (fs.existsSync(path.join(dir, "Default"))) return true;
    if (fs.existsSync(path.join(dir, "Local State"))) return true;
    const entries = fs.readdirSync(dir).filter((n) => n !== ".profile.lock");
    return entries.length > 0;
  } catch {
    return false;
  }
}

/**
 * Persistent user-data-dir for the chosen browser engine.
 * - Chrome → chrome-profile (falls back to legacy browser-profile if that already has a login)
 * - Edge → edge-profile
 * - Bundled → browser-profile
 *
 * @param {string} configuredProfileDir  typically `{appData}/browser-profile`
 * @param {BrowserLaunchTarget} target
 * @returns {string}
 */
export function resolvePersistentProfileDir(configuredProfileDir, target) {
  if (!configuredProfileDir) return "";
  const parent = profileParentDir(configuredProfileDir);
  if (!parent) return configuredProfileDir;

  const chromeDir = path.join(parent, "chrome-profile");
  const edgeDir = path.join(parent, "edge-profile");
  const legacyDir = path.join(parent, "browser-profile");

  if (target.mode === "system_edge") {
    return edgeDir;
  }
  if (target.mode === "system_chrome") {
    // Keep existing Facebook sessions that lived in browser-profile (pre-1.1.5).
    if (!hasProfileData(chromeDir) && hasProfileData(legacyDir)) {
      return legacyDir;
    }
    return chromeDir;
  }
  return legacyDir;
}

/** All profile dirs under app data (for reset). */
export function allPersistentProfileDirs(configuredProfileDir) {
  const parent = profileParentDir(configuredProfileDir);
  if (!parent) {
    return configuredProfileDir ? [configuredProfileDir] : [];
  }
  return [
    path.join(parent, "chrome-profile"),
    path.join(parent, "edge-profile"),
    path.join(parent, "browser-profile"),
  ];
}

/** Copy shown when we had to fall back to bundled Chromium. */
export function bundledBrowserDealerMessage() {
  return (
    `Install Google Chrome (${chromeInstallUrl()}) or Microsoft Edge (${edgeInstallUrl()}) ` +
    `for easiest Facebook login — then click Open Facebook again. The built-in browser often blocks passkeys.`
  );
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
 * Playwright defaults that break normal Chrome/Edge login UX:
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
