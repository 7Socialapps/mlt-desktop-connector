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
  const alive = browserPid ? isProcessAlive(browserPid) : false;
  if (context) {
    try {
      const browser = context.browser();
      if (browser && !browser.isConnected()) {
        browserState = "crashed";
      }
    } catch {
      browserState = "crashed";
    }
  }
  if (browserState === "ready" && browserPid && !alive) {
    browserState = "crashed";
  }
  if (browserState === "ready" && profileState !== "profile_initializing") {
    profileState = "profile_ready";
  } else if (browserState === "stopped") {
    profileState = inspectProfileOnDisk();
  }
  return {
    browser_state: browserState,
    pid: browserPid,
    browser_connected: Boolean(context?.browser()?.isConnected()),
    process_alive: alive,
    profile_status: profileState,
    profile_path: profileDir() || null,
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
  removeLockFile();
  profileState = inspectProfileOnDisk();

  if (previousPid) {
    emitEvent("browser_stopped", { pid: previousPid, reason });
  }
}

function attachDisconnectHandler() {
  const browser = context?.browser();
  if (!browser) {
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
}

async function handleLaunch(id) {
  if (context) {
    try {
      const browser = context.browser();
      if (browser?.isConnected()) {
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
    context = await chromium.launchPersistentContext(dir, {
      headless: false,
      args: ["--disable-dev-shm-usage"],
    });
    attachDisconnectHandler();
    browserPid = context.browser()?.process()?.pid ?? null;
    if (browserPid) {
      writeLockFile(browserPid);
    }
    const pages = context.pages();
    page = pages.length > 0 ? pages[0] : await context.newPage();
    if (page.url() === "about:blank") {
      await page.goto("about:blank");
    }
    browserState = "ready";
    profileState = "profile_ready";
    emitEvent("browser_ready", { pid: browserPid });
    ok(id, { ...currentStatus(), launched: true });
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
    await page.goto("https://www.facebook.com/", {
      waitUntil: "domcontentloaded",
      timeout: 60_000,
    });
    await page.waitForTimeout(1500);
    const detection = await runFacebookDetection();
    ok(id, {
      navigated: true,
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
  const destination = createVehicle ? "marketplace_create_vehicle" : "marketplace";
  const targetUrl = destinationUrl(destination);

  emitEvent("marketplace_status_changed", {
    status: "marketplace_loading",
  });

  try {
    await navigateWithRetry(page, targetUrl);
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
