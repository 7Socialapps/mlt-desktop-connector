#!/usr/bin/env node
/**
 * Playwright browser sidecar — long-running daemon (Milestone 2.2+).
 * Rust communicates via newline-delimited JSON on stdin/stdout.
 * Holds a single managed Chromium instance with persistent profile (2.3+).
 */
import { chromium } from "playwright";
import readline from "node:readline";
import fs from "node:fs";
import path from "node:path";
import { detectFromPage } from "./facebook-detector.mjs";
import { evaluateMarketplaceFromDetection } from "./marketplace-evaluator.mjs";
import { evaluateMessengerFromDetection } from "./messenger-evaluator.mjs";
import { evaluateNotificationsFromDetection } from "./notifications-evaluator.mjs";
import {
  destinationUrl,
  navigateWithRetry,
  waitForPageReady,
} from "./navigation.mjs";
import { verifyVehicleCreateFromPage } from "./vehicle-create-verifier.mjs";
import { fillVehicleFormFromPage } from "./marketplace/form-fill.mjs";
import { uploadVehicleImagesFromPage, retrySingleImageUpload } from "./marketplace/image-upload.mjs";
import { verifyFilledFormFromPage } from "./marketplace/form-verify.mjs";
import {
  isBrowserContextConnected,
  profileStateWhileBrowserRunning,
  resolveBrowserProcessPid,
} from "./browser-process.mjs";
import {
  browserIgnoreDefaultArgs,
  browserLaunchArgs,
  bundledBrowserDealerMessage,
  resolveBrowserLaunchTarget,
} from "./chrome-channel.mjs";

/** @typedef {"stopped"|"starting"|"ready"|"crashed"} BrowserState */
/** @typedef {"profile_missing"|"profile_initializing"|"profile_ready"|"profile_locked"|"profile_corrupt"|"profile_reset_required"} ProfileState */

/** @type {import("playwright").BrowserContext | null} */
let context = null;
/** @type {import("playwright").Page | null} */
let page = null;
/** @type {BrowserState} */
let browserState = "stopped";
/** @type {ProfileState} */
let profileState = "profile_missing";
/** @type {object | null} */
let lastFacebookDetection = null;
/** @type {number | null} */
let browserPid = null;
/** @type {import("./chrome-channel.mjs").BrowserLaunchTarget | null} */
let activeLaunchTarget = null;

const LOCK_FILE = ".profile.lock";

function profileDir() {
  return process.env.MLT_BROWSER_PROFILE_DIR ?? "";
}

function lockFilePath() {
  return path.join(profileDir(), LOCK_FILE);
}

function emit(line) {
  process.stdout.write(`${JSON.stringify(line)}\n`);
}

function fail(id, errorCode, message, extra = null) {
  emit({
    id,
    ok: false,
    error_code: errorCode,
    error: message,
    ...(extra ? { result: extra } : {}),
  });
}

function ok(id, result) {
  emit({ id, ok: true, result });
}

function emitEvent(event, data = {}) {
  emit({ event, data });
}

function isProcessAlive(pid) {
  if (!pid || typeof pid !== "number") {
    return false;
  }
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
}

function readLockFile() {
  try {
    const raw = fs.readFileSync(lockFilePath(), "utf8");
    return JSON.parse(raw);
  } catch {
    return null;
  }
}

function writeLockFile(pid) {
  const dir = profileDir();
  if (!dir) return;
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(
    lockFilePath(),
    JSON.stringify({
      pid,
      created_at: new Date().toISOString(),
      owner: "mlt-browser-sidecar",
    }),
  );
}

function removeLockFile() {
  try {
    fs.unlinkSync(lockFilePath());
  } catch {
    /* ignore */
  }
}

function inspectProfileOnDisk() {
  const dir = profileDir();
  if (!dir) {
    return "profile_missing";
  }
  if (!fs.existsSync(dir)) {
    return "profile_missing";
  }
  const lock = readLockFile();
  if (lock?.pid && isProcessAlive(lock.pid) && lock.pid !== browserPid) {
    return "profile_locked";
  }
  const hasDefault = fs.existsSync(path.join(dir, "Default"));
  const hasLocalState = fs.existsSync(path.join(dir, "Local State"));
  if (hasDefault || hasLocalState) {
    return "profile_ready";
  }
  try {
    const entries = fs.readdirSync(dir).filter((e) => e !== LOCK_FILE);
    if (entries.length > 0) {
      return "profile_ready";
    }
  } catch {
    /* ignore */
  }
  return "profile_missing";
}

function currentStatus() {
  const connected = isBrowserContextConnected(context, browserState);
  const alive = browserPid ? isProcessAlive(browserPid) : connected;

  if (context && browserState === "ready" && !connected) {
    browserState = "crashed";
  }
  if (browserState === "ready" && browserPid && !isProcessAlive(browserPid) && !connected) {
    browserState = "crashed";
  }

  const runningProfile = profileStateWhileBrowserRunning(browserState, context);
  if (runningProfile) {
    profileState = runningProfile;
  } else if (browserState === "stopped") {
    profileState = inspectProfileOnDisk();
  }

  const target = activeLaunchTarget || resolveBrowserLaunchTarget();
  return {
    browser_state: browserState,
    pid: browserPid,
    browser_connected: connected,
    process_alive: alive,
    profile_status: profileState,
    profile_path: profileDir() || null,
    browser_mode: target.mode,
    browser_label: target.label,
    browser_channel: target.channel,
  };
}

async function teardownBrowser(reason = "stop") {
  const previousPid = browserPid;
  browserState = "stopped";
  browserPid = null;

  try {
    if (context) {
      await context.close();
    }
  } catch {
    /* ignore close errors */
  }

  context = null;
  page = null;
  activeLaunchTarget = null;
  removeLockFile();
  profileState = inspectProfileOnDisk();

  if (previousPid) {
    emitEvent("browser_stopped", { pid: previousPid, reason });
  }
}

function attachDisconnectHandler() {
  try {
    const browser = context?.browser();
    if (!browser || typeof browser.on !== "function") {
      return;
    }
    browser.on("disconnected", () => {
      if (browserState === "ready" || browserState === "starting") {
        browserState = "crashed";
        emitEvent("browser_disconnected", {
          pid: browserPid,
          reason: "disconnected",
        });
      }
      context = null;
      page = null;
      browserPid = null;
      removeLockFile();
    });
  } catch {
    /* persistent context may not expose browser() — rely on explicit stop/crash detection */
  }
}

async function handleLaunch(id) {
  if (context) {
    try {
      if (isBrowserContextConnected(context, "ready")) {
        ok(id, {
          ...currentStatus(),
          already_running: true,
        });
        return;
      }
    } catch {
      /* fall through to relaunch */
    }
  }

  if (browserPid && isProcessAlive(browserPid)) {
    ok(id, {
      ...currentStatus(),
      already_running: true,
    });
    return;
  }

  await teardownBrowser("relaunch");

  const dir = profileDir();
  if (!dir) {
    fail(id, "PROFILE_DIR_MISSING", "MLT_BROWSER_PROFILE_DIR is not configured");
    return;
  }

  const diskState = inspectProfileOnDisk();
  if (diskState === "profile_locked") {
    profileState = "profile_locked";
    fail(
      id,
      "PROFILE_LOCKED",
      "Browser profile is locked by another process",
    );
    return;
  }

  browserState = "starting";
  profileState =
    diskState === "profile_missing" ? "profile_initializing" : "profile_ready";
  emitEvent("browser_starting", {});

  try {
    fs.mkdirSync(dir, { recursive: true });
    const launchTarget = resolveBrowserLaunchTarget();
    activeLaunchTarget = launchTarget;

    /** @type {import("playwright").LaunchPersistentContextOptions} */
    const launchOptions = {
      headless: false,
      viewport: { width: 1280, height: 900 },
      ignoreDefaultArgs: browserIgnoreDefaultArgs(),
      args: browserLaunchArgs(),
    };

    let chromiumPath = launchTarget.executable_path;
    if (launchTarget.channel) {
      // Real Google Chrome / Edge + dedicated MLT user-data-dir (not Chrome for Testing).
      launchOptions.channel = launchTarget.channel;
    } else {
      const executablePath = chromium.executablePath();
      if (!executablePath || !fs.existsSync(executablePath)) {
        fail(
          id,
          "CHROMIUM_NOT_INSTALLED",
          bundledBrowserDealerMessage(),
        );
        return;
      }
      // Fallback only — dealers should install Chrome for passkeys / normal 2FA.
      emitEvent("browser_using_bundled_fallback", {
        message: bundledBrowserDealerMessage(),
      });
      chromiumPath = executablePath;
      launchOptions.executablePath = executablePath;
      // Gatekeeper quarantine on first-run / DMG-copied Chromium blocks launch.
      if (process.platform === "darwin") {
        try {
          const { spawnSync } = await import("node:child_process");
          spawnSync("xattr", ["-cr", executablePath], { stdio: "ignore" });
          const browsersRoot = process.env.PLAYWRIGHT_BROWSERS_PATH;
          if (browsersRoot) {
            spawnSync("xattr", ["-cr", browsersRoot], { stdio: "ignore" });
          }
        } catch {
          /* best-effort */
        }
      }
    }

    // Headed, large window — dealers must see Facebook login immediately.
    // Persistent userDataDir keeps Facebook login across Connector restarts.
    context = await chromium.launchPersistentContext(dir, launchOptions);
    attachDisconnectHandler();

    // Soften the most obvious webdriver fingerprint (Meta may still require 2FA).
    try {
      await context.addInitScript(() => {
        try {
          Object.defineProperty(navigator, "webdriver", {
            get: () => undefined,
          });
        } catch {
          /* ignore */
        }
      });
    } catch {
      /* ignore */
    }

    browserPid = resolveBrowserProcessPid(context);
    if (!browserPid) {
      browserPid = await resolveChromiumPidFallback(dir, launchTarget);
    }
    if (browserPid) {
      writeLockFile(browserPid);
    }
    const pages = context.pages();
    page = pages.length > 0 ? pages[0] : await context.newPage();
    await page.setViewportSize({ width: 1280, height: 900 }).catch(() => {});
    await activateChromiumWindow(chromiumPath, browserPid, launchTarget);
    browserState = "ready";
    profileState = "profile_ready";
    emitEvent("browser_ready", {
      pid: browserPid,
      headed: true,
      chromium_path: chromiumPath,
      browser_mode: launchTarget.mode,
      browser_label: launchTarget.label,
    });
    ok(id, {
      ...currentStatus(),
      launched: true,
      headed: true,
      pid: browserPid,
      chromium_path: chromiumPath,
      browser_mode: launchTarget.mode,
      browser_label: launchTarget.label,
      current_url: page.url(),
      page_title: await page.title().catch(() => ""),
    });
  } catch (err) {
    browserState = "stopped";
    browserPid = null;
    context = null;
    page = null;
    removeLockFile();
    const message = err instanceof Error ? err.message : String(err);
    if (
      message.includes("SingletonLock") ||
      message.includes("profile is already in use") ||
      message.includes("ProcessSingleton")
    ) {
      profileState = "profile_locked";
      fail(id, "PROFILE_LOCKED", "Browser profile is locked by another process");
    } else if (
      message.includes("corrupt") ||
      message.includes("cannot read") ||
      message.includes("Failed to create")
    ) {
      profileState = "profile_corrupt";
      fail(id, "PROFILE_CORRUPT", "Browser profile appears corrupt");
    } else {
      profileState = inspectProfileOnDisk();
      fail(id, "LAUNCH_FAILED", message);
    }
  }
}

async function handleStop(id) {
  await teardownBrowser("stop");
  ok(id, { ...currentStatus(), stopped: true });
}

async function handleRestart(id) {
  await teardownBrowser("restart");
  await handleLaunch(id);
}

async function handlePing(id) {
  ok(id, { alive: true, ...currentStatus() });
}

async function handleStatus(id) {
  ok(id, currentStatus());
}

async function handleProfileStatus(id) {
  profileState = inspectProfileOnDisk();
  ok(id, {
    profile_status: profileState,
    profile_path: profileDir() || null,
    browser_state: browserState,
  });
}

async function runFacebookDetection() {
  if (!page || browserState !== "ready") {
    return null;
  }
  try {
    const detection = await detectFromPage(page);
    lastFacebookDetection = detection;
    emitEvent("facebook_session_changed", {
      state: detection.state,
      reason_code: detection.reason_code,
      marketplace_accessible: detection.marketplace_accessible,
    });
    return detection;
  } catch {
    return null;
  }
}

async function handleOpenFacebookLogin(id) {
  if (!page || browserState !== "ready") {
    fail(id, "NO_ACTIVE_PAGE", "Browser is not ready — launch the browser first");
    return;
  }

  lastFacebookDetection = {
    state: "facebook_login_in_progress",
    checked_at: new Date().toISOString(),
    current_url: page.url(),
    marketplace_accessible: false,
    reason_code: "navigation_started",
  };
  emitEvent("facebook_session_changed", {
    state: "facebook_login_in_progress",
    reason_code: "navigation_started",
  });

  try {
    await page.bringToFront().catch(() => {});
    await page.goto("https://www.facebook.com/", {
      waitUntil: "domcontentloaded",
      timeout: 60_000,
    });
    await page.waitForTimeout(1500);
    const target = activeLaunchTarget || resolveBrowserLaunchTarget();
    const executablePath =
      target.executable_path ||
      (() => {
        try {
          return chromium.executablePath();
        } catch {
          return null;
        }
      })();
    await activateChromiumWindow(executablePath, browserPid, target);
    const detection = await runFacebookDetection();
    ok(id, {
      navigated: true,
      headed: true,
      pid: browserPid,
      chromium_path: executablePath,
      browser_mode: target.mode,
      browser_label: target.label,
      facebook: detection ?? lastFacebookDetection,
    });
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    lastFacebookDetection = {
      state: "facebook_error",
      checked_at: new Date().toISOString(),
      current_url: page.url(),
      marketplace_accessible: false,
      reason_code: "navigation_failed",
    };
    fail(id, "FACEBOOK_NAV_FAILED", message);
  }
}

/**
 * Force the headed browser window onto the user's screen (macOS Dock bounce).
 * Playwright page.bringToFront alone is often invisible behind the connector.
 * @param {string | null | undefined} executablePath
 * @param {number | null | undefined} pid
 * @param {import("./chrome-channel.mjs").BrowserLaunchTarget | null | undefined} launchTarget
 */
async function activateChromiumWindow(executablePath, pid, launchTarget) {
  if (process.platform !== "darwin") {
    try {
      await page?.bringToFront();
    } catch {
      /* ignore */
    }
    return;
  }
  const target = launchTarget || activeLaunchTarget || resolveBrowserLaunchTarget();
  const processHint = target.process_name_hint || "Chrome";
  try {
    const { spawnSync } = await import("node:child_process");
    let appBundle = null;
    if (executablePath && executablePath.includes(".app/")) {
      appBundle = executablePath.slice(0, executablePath.indexOf(".app/") + 4);
    }
    if (appBundle && fs.existsSync(appBundle)) {
      // open -a brings the app forward and makes Dock show a bounce.
      spawnSync("open", ["-a", appBundle], { stdio: "ignore" });
    } else if (target.mode === "system_chrome") {
      spawnSync("open", ["-a", "Google Chrome"], { stdio: "ignore" });
    } else if (target.mode === "system_edge") {
      spawnSync("open", ["-a", "Microsoft Edge"], { stdio: "ignore" });
    }
    // Prefer PID when known — system Chrome may have other windows open.
    if (pid) {
      spawnSync(
        "osascript",
        [
          "-e",
          `tell application "System Events" to set frontmost of (first process whose unix id is ${Number(pid)}) to true`,
        ],
        { stdio: "ignore" },
      );
    } else {
      const escaped = processHint.replace(/"/g, "");
      const script = [
        'tell application "System Events"',
        `  set procs to every process whose name contains "${escaped}"`,
        "  repeat with p in procs",
        "    set frontmost of p to true",
        "  end repeat",
        "end tell",
      ].join("\n");
      spawnSync("osascript", ["-e", script], { stdio: "ignore" });
    }
  } catch {
    /* best-effort visibility */
  }
  try {
    await page?.bringToFront();
  } catch {
    /* ignore */
  }
}

/**
 * Find our headed browser PID by the dedicated MLT user-data-dir (safe with system Chrome).
 * @param {string} profilePath
 * @param {import("./chrome-channel.mjs").BrowserLaunchTarget | null | undefined} launchTarget
 */
async function resolveChromiumPidFallback(profilePath, launchTarget) {
  if (process.platform !== "darwin" || !profilePath) {
    return null;
  }
  try {
    const { spawnSync } = await import("node:child_process");
    // Match our persistent profile path so we never grab the dealer's everyday Chrome.
    const result = spawnSync("pgrep", ["-f", profilePath], {
      encoding: "utf8",
    });
    if (result.status === 0 && result.stdout?.trim()) {
      const pids = result.stdout
        .trim()
        .split(/\s+/)
        .map((s) => Number(s))
        .filter((n) => Number.isFinite(n) && n > 0);
      if (pids[0]) return pids[0];
    }
    const target = launchTarget || activeLaunchTarget || resolveBrowserLaunchTarget();
    if (target.mode === "bundled_chromium") {
      const testing = spawnSync("pgrep", ["-f", "Google Chrome for Testing"], {
        encoding: "utf8",
      });
      if (testing.status === 0 && testing.stdout?.trim()) {
        const pids = testing.stdout
          .trim()
          .split(/\s+/)
          .map((s) => Number(s))
          .filter((n) => Number.isFinite(n) && n > 0);
        return pids[0] ?? null;
      }
    }
    return null;
  } catch {
    return null;
  }
}

function diagnosticsDir() {
  return process.env.MLT_BROWSER_DIAGNOSTICS_DIR ?? "";
}

async function captureDiagnosticScreenshot(label) {
  const dir = diagnosticsDir();
  if (!dir || !page) return null;
  try {
    fs.mkdirSync(dir, { recursive: true });
    const filename = `marketplace-failure-${label}-${Date.now()}.png`;
    const filepath = path.join(dir, filename);
    await page.screenshot({ path: filepath, fullPage: false });
    return filepath;
  } catch {
    return null;
  }
}

async function evaluateMarketplaceState() {
  const fb = await detectFromPage(page);
  const url = page.url();
  return evaluateMarketplaceFromDetection(fb, url);
}

async function handleNavigate(id, params = {}) {
  if (!page || browserState !== "ready") {
    fail(id, "NO_ACTIVE_PAGE", "Browser is not ready — launch the browser first");
    return;
  }

  const destination = params?.destination;
  const targetUrl = destinationUrl(destination);
  if (!targetUrl) {
    fail(id, "INVALID_DESTINATION", `Unknown navigation destination: ${destination}`);
    return;
  }

  try {
    const nav = await navigateWithRetry(page, targetUrl);
    const readiness = await waitForPageReady(page);
    const fb = await runFacebookDetection();
    ok(id, {
      navigated: true,
      destination,
      attempt: nav.attempt,
      current_url: nav.current_url,
      page_title: readiness.title,
      redirect_detected: nav.redirect_detected,
      facebook: fb ?? lastFacebookDetection,
    });
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    fail(id, "NAVIGATION_FAILED", message, {
      destination,
      current_url: page.url(),
    });
  }
}

async function handleOpenMessenger(id) {
  if (!page || browserState !== "ready") {
    fail(id, "NO_ACTIVE_PAGE", "Browser is not ready — launch the browser first");
    return;
  }

  try {
    const targetUrl = destinationUrl("messenger");
    await navigateWithRetry(page, targetUrl);
    await waitForPageReady(page);
    const fb = (await runFacebookDetection()) ?? lastFacebookDetection ?? {
      state: "facebook_not_checked",
      reason_code: "no_detection",
    };
    const url = page.url();
    const evaluation = evaluateMessengerFromDetection(fb, url);
    const checked_at = new Date().toISOString();
    ok(id, {
      messenger: { ...evaluation, checked_at },
      navigated: true,
    });
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    fail(id, "MESSENGER_NAV_FAILED", message);
  }
}

async function handleOpenNotifications(id) {
  if (!page || browserState !== "ready") {
    fail(id, "NO_ACTIVE_PAGE", "Browser is not ready — launch the browser first");
    return;
  }

  try {
    const targetUrl = destinationUrl("notifications");
    await navigateWithRetry(page, targetUrl);
    await waitForPageReady(page);
    const fb = (await runFacebookDetection()) ?? lastFacebookDetection ?? {
      state: "facebook_not_checked",
      reason_code: "no_detection",
    };
    const url = page.url();
    const evaluation = evaluateNotificationsFromDetection(fb, url);
    const checked_at = new Date().toISOString();
    ok(id, {
      notifications: { ...evaluation, checked_at },
      navigated: true,
    });
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    fail(id, "NOTIFICATIONS_NAV_FAILED", message);
  }
}

async function handleOpenMarketplace(id, params = {}) {
  if (!page || browserState !== "ready") {
    fail(id, "NO_ACTIVE_PAGE", "Browser is not ready — launch the browser first");
    return;
  }

  const createVehicle = Boolean(params?.create_vehicle);
  const skipNavigation = Boolean(params?.skip_navigation);
  const destination = createVehicle ? "marketplace_create_vehicle" : "marketplace";
  const targetUrl = destinationUrl(destination);

  emitEvent("marketplace_status_changed", {
    status: "marketplace_loading",
  });

  try {
    if (!skipNavigation) {
      await navigateWithRetry(page, targetUrl);
    }
    const evaluation = await evaluateMarketplaceState();
    const checked_at = new Date().toISOString();
    let screenshot_path = null;

    if (
      evaluation.status !== "marketplace_ready" &&
      evaluation.status !== "marketplace_login_required"
    ) {
      screenshot_path = await captureDiagnosticScreenshot(evaluation.status);
    }

    const result = {
      ...evaluation,
      checked_at,
      screenshot_path,
    };

    emitEvent("marketplace_status_changed", {
      status: evaluation.status,
      reason_code: evaluation.reason_code,
    });

    if (evaluation.status === "marketplace_error") {
      fail(id, "MARKETPLACE_NAV_FAILED", "Marketplace navigation failed", result);
      return;
    }

    ok(id, { marketplace: result, navigated: true });
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    const screenshot_path = await captureDiagnosticScreenshot("navigation_error");
    const result = {
      status: "marketplace_error",
      reason_code: "navigation_timeout",
      current_url: page.url(),
      checked_at: new Date().toISOString(),
      screenshot_path,
    };
    emitEvent("marketplace_status_changed", {
      status: "marketplace_error",
      reason_code: "navigation_timeout",
    });
    fail(id, "MARKETPLACE_NAV_FAILED", message);
  }
}

async function handleDetectFacebookSession(id) {
  if (!page || browserState !== "ready") {
    fail(id, "NO_ACTIVE_PAGE", "Browser is not ready");
    return;
  }
  const detection = await runFacebookDetection();
  if (!detection) {
    fail(id, "DETECTION_FAILED", "Facebook session detection failed");
    return;
  }
  ok(id, { facebook: detection });
}

async function handleFillVehicleForm(id, params = {}) {
  if (!page || browserState !== "ready") {
    fail(id, "NO_ACTIVE_PAGE", "Browser is not ready — launch the browser first");
    return;
  }

  const payload = params?.payload ?? params?.fields ?? {};
  try {
    const result = await fillVehicleFormFromPage(page, payload);
    ok(id, { form_fill: result });
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    fail(id, "FORM_FILL_FAILED", message);
  }
}

async function handleUploadVehicleImages(id, params = {}) {
  if (!page || browserState !== "ready") {
    fail(id, "NO_ACTIVE_PAGE", "Browser is not ready — launch the browser first");
    return;
  }

  const images = Array.isArray(params?.images) ? params.images : [];
  try {
    let result = await uploadVehicleImagesFromPage(page, images);

    const failed = result.uploaded.filter((u) => !u.ok);
    for (const entry of failed) {
      const img = images.find((i) => i.index === entry.index);
      if (!img?.local_path) continue;
      const retry = await retrySingleImageUpload(page, img.local_path);
      entry.ok = retry.ok;
      entry.reason = retry.ok ? undefined : "retry_failed";
      entry.attempts = (entry.attempts ?? 1) + 1;
    }

    result = {
      ...result,
      uploaded: result.uploaded,
      thumbnail_count: result.thumbnail_count,
    };

    ok(id, { image_upload: result });
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    fail(id, "IMAGE_UPLOAD_FAILED", message);
  }
}

async function handleVerifyFilledForm(id, params = {}) {
  if (!page || browserState !== "ready") {
    fail(id, "NO_ACTIVE_PAGE", "Browser is not ready — launch the browser first");
    return;
  }

  const expectedValues = params?.expected_values ?? params?.payload ?? {};
  const expectedImageCount = Number(params?.expected_image_count ?? 0);

  try {
    const report = await verifyFilledFormFromPage(page, expectedValues, expectedImageCount);
    let screenshot_path = null;
    if (!report.ready) {
      screenshot_path = await captureDiagnosticScreenshot(report.reason_code ?? "verify_failed");
    }
    ok(id, {
      form_verification: {
        ...report,
        screenshot_path,
      },
    });
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    const screenshot_path = await captureDiagnosticScreenshot("verify_filled_error");
    fail(id, "FORM_VERIFY_FAILED", message, {
      form_verification: {
        ready: false,
        reason_code: "verify_error",
        screenshot_path,
        current_url: page.url(),
        checked_at: new Date().toISOString(),
      },
    });
  }
}

async function handleBringBrowserForward(id) {
  if (!page || browserState !== "ready") {
    fail(id, "NO_ACTIVE_PAGE", "Browser is not ready");
    return;
  }
  try {
    const target = activeLaunchTarget || resolveBrowserLaunchTarget();
    const executablePath =
      target.executable_path ||
      (() => {
        try {
          return chromium.executablePath();
        } catch {
          return null;
        }
      })();
    await activateChromiumWindow(executablePath, browserPid, target);
    ok(id, {
      brought_forward: true,
      pid: browserPid,
      headed: true,
      browser_label: target.label,
      current_url: page.url(),
    });
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    fail(id, "BRING_FORWARD_FAILED", message);
  }
}

async function handleVerifyVehicleCreate(id) {
  if (!page || browserState !== "ready") {
    fail(id, "NO_ACTIVE_PAGE", "Browser is not ready — launch the browser first");
    return;
  }

  try {
    const verification = await verifyVehicleCreateFromPage(page);
    let screenshot_path = null;
    if (!verification.ready) {
      screenshot_path = await captureDiagnosticScreenshot(
        verification.reason_code ?? "verify_failed",
      );
    }
    ok(id, {
      vehicle_create: {
        ...verification,
        screenshot_path,
      },
    });
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    const screenshot_path = await captureDiagnosticScreenshot("verify_error");
    fail(id, "VEHICLE_CREATE_VERIFY_FAILED", message, {
      vehicle_create: {
        ready: false,
        reason_code: "verify_error",
        screenshot_path,
        current_url: page.url(),
        checked_at: new Date().toISOString(),
      },
    });
  }
}

async function handleGetActivePage(id) {
  if (!page || browserState !== "ready") {
    fail(id, "NO_ACTIVE_PAGE", "Browser is not ready");
    return;
  }
  let url = "about:blank";
  let title = "";
  try {
    url = page.url();
    title = await page.title();
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    fail(id, "PAGE_READ_FAILED", message);
    return;
  }
  ok(id, {
    url,
    title,
    pid: browserPid,
  });
}

async function handleShutdown(id) {
  await teardownBrowser("shutdown");
  ok(id, { shutting_down: true });
  emitEvent("daemon_shutdown", {});
  setTimeout(() => process.exit(0), 10);
}

async function dispatch(line) {
  let msg;
  try {
    msg = JSON.parse(line);
  } catch {
    emit({
      ok: false,
      error_code: "INVALID_JSON",
      error: "Failed to parse JSON request",
    });
    return;
  }

  const { id, method, params } = msg;
  if (!id || !method) {
    emit({
      ok: false,
      error_code: "INVALID_REQUEST",
      error: "Request requires id and method",
    });
    return;
  }

  try {
    switch (method) {
      case "ping":
        await handlePing(id);
        break;
      case "launch":
        await handleLaunch(id);
        break;
      case "stop":
        await handleStop(id);
        break;
      case "restart":
        await handleRestart(id);
        break;
      case "status":
        await handleStatus(id);
        break;
      case "profile_status":
        await handleProfileStatus(id);
        break;
      case "open_facebook_login":
        await handleOpenFacebookLogin(id);
        break;
      case "detect_facebook_session":
        await handleDetectFacebookSession(id);
        break;
      case "open_marketplace":
        await handleOpenMarketplace(id, params ?? {});
        break;
      case "verify_vehicle_create":
        await handleVerifyVehicleCreate(id);
        break;
      case "fill_vehicle_form":
        await handleFillVehicleForm(id, params ?? {});
        break;
      case "upload_vehicle_images":
        await handleUploadVehicleImages(id, params ?? {});
        break;
      case "verify_filled_form":
        await handleVerifyFilledForm(id, params ?? {});
        break;
      case "bring_browser_forward":
        await handleBringBrowserForward(id);
        break;
      case "navigate":
        await handleNavigate(id, params ?? {});
        break;
      case "open_messenger":
        await handleOpenMessenger(id);
        break;
      case "open_notifications":
        await handleOpenNotifications(id);
        break;
      case "get_active_page":
        await handleGetActivePage(id);
        break;
      case "shutdown":
        await handleShutdown(id);
        break;
      default:
        fail(id, "UNKNOWN_METHOD", `Unknown method: ${method}`);
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    fail(id, "METHOD_FAILED", message);
  }
}

const rl = readline.createInterface({
  input: process.stdin,
  terminal: false,
});

rl.on("line", (line) => {
  void dispatch(line.trim());
});

rl.on("close", () => {
  void teardownBrowser("stdin_closed").finally(() => process.exit(0));
});

process.on("SIGTERM", () => {
  void teardownBrowser("sigterm").finally(() => process.exit(0));
});

process.on("SIGINT", () => {
  void teardownBrowser("sigint").finally(() => process.exit(0));
});

emitEvent("ready", { pid: process.pid });
