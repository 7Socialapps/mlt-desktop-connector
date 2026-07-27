#!/usr/bin/env node
/**
 * Playwright browser sidecar — CLI mode (Milestone 2.1).
 * Invoked by the Rust connector; stdout is a single JSON object per command.
 * Does not auto-download Chromium on detect — use `npm run browser:install` explicitly.
 */
import { chromium } from "playwright";
import { createRequire } from "node:module";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const require = createRequire(import.meta.url);

function testStatePath() {
  return (
    process.env.MLT_BROWSER_TEST_STATE_FILE ??
    path.join(__dirname, ".browser-test-state.json")
  );
}

function readTestState() {
  try {
    const raw = fs.readFileSync(testStatePath(), "utf8");
    return JSON.parse(raw);
  } catch {
    return null;
  }
}

function writeTestState(state) {
  fs.writeFileSync(testStatePath(), JSON.stringify(state));
}

function clearTestState() {
  try {
    fs.unlinkSync(testStatePath());
  } catch {
    /* ignore */
  }
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

function emit(result) {
  process.stdout.write(`${JSON.stringify(result)}\n`);
}

function fail(errorCode, message) {
  emit({ ok: false, error_code: errorCode, error: message });
  process.exit(1);
}

function playwrightPackageVersion() {
  try {
    const pkg = require("playwright/package.json");
    return pkg.version ?? null;
  } catch {
    return null;
  }
}

async function detectRuntime() {
  const playwrightVersion = playwrightPackageVersion();
  const result = {
    ok: true,
    playwright_installed: Boolean(playwrightVersion),
    playwright_version: playwrightVersion,
    chromium_installed: false,
    chromium_path: null,
    node_version: process.version,
  };

  if (!playwrightVersion) {
    return { ...result, ok: true, chromium_installed: false };
  }

  try {
    const chromiumPath = chromium.executablePath();
    result.chromium_installed = Boolean(chromiumPath);
    result.chromium_path = chromiumPath ?? null;
  } catch (err) {
    result.chromium_installed = false;
    result.detect_error = err instanceof Error ? err.message : String(err);
  }

  return result;
}

async function launchTest() {
  const existing = readTestState();
  if (existing?.pid && isProcessAlive(existing.pid)) {
    return { ok: true, already_open: true, pid: existing.pid };
  }
  clearTestState();

  const browser = await chromium.launch({
    headless: false,
    args: ["--disable-dev-shm-usage"],
  });
  const pid =
    typeof browser.process === "function" ? (browser.process()?.pid ?? null) : null;
  if (pid) {
    writeTestState({ pid, launched_at: new Date().toISOString() });
  }
  const context = await browser.newContext();
  const page = await context.newPage();
  await page.goto("about:blank");
  // Detach — test browser stays open until close-test kills by PID.
  browser.on("disconnected", () => clearTestState());
  return { ok: true, launched: true, pid };
}

async function closeTest() {
  const state = readTestState();
  if (!state?.pid) {
    return { ok: true, already_closed: true };
  }
  if (isProcessAlive(state.pid)) {
    process.kill(state.pid, "SIGTERM");
  }
  clearTestState();
  return { ok: true, closed: true, pid: state.pid };
}

async function main() {
  const command = process.argv[2] ?? "detect";

  try {
    switch (command) {
      case "detect":
        emit(await detectRuntime());
        break;
      case "launch-test":
        emit(await launchTest());
        break;
      case "close-test":
        emit(await closeTest());
        break;
      default:
        fail("UNKNOWN_COMMAND", `Unknown sidecar command: ${command}`);
    }
  } catch (err) {
    fail(
      "SIDECAR_COMMAND_FAILED",
      err instanceof Error ? err.message : String(err),
    );
  }
}

void main();
