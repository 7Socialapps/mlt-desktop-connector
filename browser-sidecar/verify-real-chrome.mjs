/**
 * One-shot: prove launchPersistentContext({ channel: 'chrome' }) opens real Google Chrome
 * (not "Chrome for Testing") with a dedicated user-data-dir.
 *
 * Usage: node browser-sidecar/verify-real-chrome.mjs
 * Exit 0 on success; prints PROCESS/MODE lines for operators.
 */
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { chromium } from "playwright";
import {
  browserIgnoreDefaultArgs,
  browserLaunchArgs,
  resolveBrowserLaunchTarget,
} from "./chrome-channel.mjs";

const target = resolveBrowserLaunchTarget();
console.log("TARGET", JSON.stringify(target));

if (target.mode !== "system_chrome") {
  console.error("FAIL: Google Chrome not found — install from https://www.google.com/chrome/");
  process.exit(2);
}

const dir = fs.mkdtempSync(path.join(os.tmpdir(), "mlt-real-chrome-"));
console.log("PROFILE", dir);

let context;
try {
  context = await chromium.launchPersistentContext(dir, {
    channel: "chrome",
    headless: false,
    viewport: { width: 1100, height: 800 },
    ignoreDefaultArgs: browserIgnoreDefaultArgs(),
    args: browserLaunchArgs(),
  });
  const page = context.pages()[0] || (await context.newPage());
  await page.goto("https://www.facebook.com/", {
    waitUntil: "domcontentloaded",
    timeout: 60_000,
  });
  await page.waitForTimeout(1500);

  const title = await page.title().catch(() => "");
  console.log("TITLE", title);

  const pgrep = spawnSync("pgrep", ["-lf", dir], { encoding: "utf8" });
  const procs = (pgrep.stdout || "").trim();
  console.log("PROCESS", procs || "(none)");

  assert.ok(procs, "expected a process matching our user-data-dir");
  assert.ok(
    !/Chrome for Testing/i.test(procs),
    "must not launch Chrome for Testing",
  );
  assert.ok(
    /Google Chrome|chrome/i.test(procs),
    "expected Google Chrome in process list",
  );

  console.log("OK real Google Chrome launched with persistent MLT profile");
} finally {
  try {
    await context?.close();
  } catch {
    /* ignore */
  }
  try {
    fs.rmSync(dir, { recursive: true, force: true });
  } catch {
    /* ignore */
  }
}
