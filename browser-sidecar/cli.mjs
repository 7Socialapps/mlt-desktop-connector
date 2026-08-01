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
    browsers_path: process.env.PLAYWRIGHT_BROWSERS_PATH ?? null,
  };

  if (!playwrightVersion) {
    return { ...result, ok: true, chromium_installed: false };
  }

  try {
    // executablePath() returns the *expected* location even when missing —
    // always verify on disk or Open Facebook skips install and launch fails.
    const chromiumPath = chromium.executablePath();
    const exists = Boolean(chromiumPath) && fs.existsSync(chromiumPath);
    result.chromium_installed = exists;
    result.chromium_path = exists ? chromiumPath : chromiumPath || null;
    if (!exists && chromiumPath) {
      result.detect_error = `Chromium binary missing at ${chromiumPath}`;
    }
  } catch (err) {
    result.chromium_installed = false;
    result.detect_error = err instanceof Error ? err.message : String(err);
  }

  return result;
}

async function launchTest() {
  const existing = readTestState();
  if (existing?.pid && isProcessAlive(existing.pid)) {
    return {
      ok: true,
      already_open: true,
      pid: existing.pid,
      headed: true,
    };
  }
  clearTestState();

  const executablePath = chromium.executablePath();
  if (!executablePath || !fs.existsSync(executablePath)) {
    fail(
      "CHROMIUM_NOT_INSTALLED",
      `Chromium is not installed at ${executablePath || "(unknown path)"}`,
    );
  }
  clearQuarantine(executablePath);

  const browser = await chromium.launch({
    headless: false,
    executablePath,
    args: [
      "--disable-dev-shm-usage",
      "--new-window",
      "--window-size=1280,900",
      "--window-position=80,60",
    ],
  });
  const proc = typeof browser.process === "function" ? browser.process() : null;
  let pid = proc?.pid ?? null;
  const context = await browser.newContext({
    viewport: { width: 1280, height: 900 },
  });
  const page = await context.newPage();
  await page.goto("about:blank");
  await page.bringToFront().catch(() => {});

  // macOS: force the window onto screen (Dock bounce) before we detach.
  if (process.platform === "darwin" && executablePath.includes(".app/")) {
    try {
      const { spawnSync } = require("node:child_process");
      const appBundle = executablePath.slice(
        0,
        executablePath.indexOf(".app/") + 4,
      );
      spawnSync("open", ["-a", appBundle], { stdio: "ignore" });
    } catch {
      /* best-effort */
    }
  }

  // Require a live PID — prior releases reported launched:true with pid:null
  // while Chromium never stayed on screen (false "verified" builds).
  if (!pid) {
    try {
      const { spawnSync } = require("node:child_process");
      const result = spawnSync("pgrep", ["-f", "Google Chrome for Testing"], {
        encoding: "utf8",
      });
      const first = result.stdout?.trim().split(/\s+/).find(Boolean);
      if (first) pid = Number(first);
    } catch {
      /* ignore */
    }
  }
  if (!pid || !isProcessAlive(pid)) {
    await browser.close().catch(() => {});
    fail(
      "LAUNCH_TEST_NO_PID",
      "Chromium launched but no living process was found — headed window did not stay open",
    );
  }

  writeTestState({ pid, launched_at: new Date().toISOString(), headed: true });
  browser.on("disconnected", () => clearTestState());
  // Keep Chromium alive after CLI exits: detach from Node's process group.
  if (proc && typeof proc.unref === "function") {
    proc.unref();
  }
  return {
    ok: true,
    launched: true,
    pid,
    headed: true,
    chromium_path: executablePath,
  };
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

function clearQuarantine(targetPath) {
  if (process.platform !== "darwin" || !targetPath || !fs.existsSync(targetPath)) {
    return;
  }
  try {
    const { spawnSync } = require("node:child_process");
    spawnSync("xattr", ["-cr", targetPath], { stdio: "ignore" });
  } catch {
    /* best-effort — Gatekeeper may still prompt */
  }
}

async function installChromium() {
  const { spawn } = await import("node:child_process");
  const playwrightCli = require.resolve("playwright/cli.js");

  return new Promise((resolve) => {
    const child = spawn(process.execPath, [playwrightCli, "install", "chromium"], {
      stdio: ["ignore", "pipe", "pipe"],
      env: process.env,
    });

    let stderr = "";
    child.stderr.on("data", (chunk) => {
      stderr += chunk.toString();
    });

    child.on("close", (code) => {
      if (code === 0) {
        const browsersPath =
          process.env.PLAYWRIGHT_BROWSERS_PATH ||
          path.dirname(path.dirname(chromium.executablePath()));
        clearQuarantine(browsersPath);
        try {
          clearQuarantine(chromium.executablePath());
        } catch {
          /* ignore */
        }
        emit({ ok: true, installed: true });
        resolve();
        return;
      }
      emit({
        ok: false,
        error_code: "CHROMIUM_INSTALL_FAILED",
        error: stderr.trim() || `playwright install exited with code ${code}`,
      });
      resolve();
    });

    child.on("error", (err) => {
      emit({
        ok: false,
        error_code: "CHROMIUM_INSTALL_FAILED",
        error: err instanceof Error ? err.message : String(err),
      });
      resolve();
    });
  });
}

async function main() {
  const command = process.argv[2] ?? "detect";

  try {
    switch (command) {
      case "detect":
        emit(await detectRuntime());
        break;
      case "install-chromium":
        await installChromium();
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

void main().finally(() => {
  // launch-test keeps Chromium alive via unref'd child; force CLI exit so
  // Rust `run_sidecar_command` is not stuck waiting on an open handle.
  // Brief delay lets the headed window map before this parent exits.
  if ((process.argv[2] ?? "") === "launch-test") {
    setTimeout(() => process.exit(0), 750);
  }
});
