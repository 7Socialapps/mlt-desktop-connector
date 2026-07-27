#!/usr/bin/env node
/**
 * Playwright browser sidecar — long-running daemon (Milestone 2.2).
 * Rust communicates via newline-delimited JSON on stdin/stdout.
 * Holds a single managed Chromium instance across commands.
 */
import { chromium } from "playwright";
import readline from "node:readline";

/** @typedef {"stopped"|"starting"|"ready"|"crashed"} BrowserState */

/** @type {import("playwright").Browser | null} */
let browser = null;
/** @type {import("playwright").BrowserContext | null} */
let context = null;
/** @type {import("playwright").Page | null} */
let page = null;
/** @type {BrowserState} */
let browserState = "stopped";
/** @type {number | null} */
let browserPid = null;

function emit(line) {
  process.stdout.write(`${JSON.stringify(line)}\n`);
}

function fail(id, errorCode, message) {
  emit({
    id,
    ok: false,
    error_code: errorCode,
    error: message,
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

function currentStatus() {
  const alive = browserPid ? isProcessAlive(browserPid) : false;
  if (browserState === "ready" && browser && !browser.isConnected()) {
    browserState = "crashed";
  }
  if (browserState === "ready" && browserPid && !alive) {
    browserState = "crashed";
  }
  return {
    browser_state: browserState,
    pid: browserPid,
    browser_connected: Boolean(browser?.isConnected()),
    process_alive: alive,
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
  try {
    if (browser) {
      await browser.close();
    }
  } catch {
    /* ignore close errors */
  }

  context = null;
  page = null;
  browser = null;

  if (previousPid) {
    emitEvent("browser_stopped", { pid: previousPid, reason });
  }
}

function attachDisconnectHandler() {
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
    browser = null;
    context = null;
    page = null;
    browserPid = null;
  });
}

async function handleLaunch(id) {
  if (browser && browser.isConnected()) {
    ok(id, {
      ...currentStatus(),
      already_running: true,
    });
    return;
  }

  if (browserPid && isProcessAlive(browserPid)) {
    ok(id, {
      ...currentStatus(),
      already_running: true,
    });
    return;
  }

  await teardownBrowser("relaunch");

  browserState = "starting";
  emitEvent("browser_starting", {});

  try {
    browser = await chromium.launch({
      headless: false,
      args: ["--disable-dev-shm-usage"],
    });
    attachDisconnectHandler();
    browserPid = browser.process()?.pid ?? null;
    context = await browser.newContext();
    page = await context.newPage();
    await page.goto("about:blank");
    browserState = "ready";
    emitEvent("browser_ready", { pid: browserPid });
    ok(id, { ...currentStatus(), launched: true });
  } catch (err) {
    browserState = "stopped";
    browserPid = null;
    browser = null;
    context = null;
    page = null;
    const message = err instanceof Error ? err.message : String(err);
    fail(id, "LAUNCH_FAILED", message);
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

  const { id, method } = msg;
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
