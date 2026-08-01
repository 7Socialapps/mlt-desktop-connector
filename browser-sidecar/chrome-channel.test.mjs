/**
 * Prefer system Chrome/Edge when installed; honor MLT_FORCE_BUNDLED_BROWSER.
 * Per-engine profile dirs must not clash.
 */
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import {
  allPersistentProfileDirs,
  browserIgnoreDefaultArgs,
  browserLaunchArgs,
  bundledBrowserDealerMessage,
  resolveBrowserLaunchTarget,
  resolvePersistentProfileDir,
} from "./chrome-channel.mjs";

const prev = process.env.MLT_FORCE_BUNDLED_BROWSER;
process.env.MLT_FORCE_BUNDLED_BROWSER = "1";
const forced = resolveBrowserLaunchTarget();
assert.equal(forced.mode, "bundled_chromium");
assert.equal(forced.channel, null);
assert.match(forced.label, /Chrome|Edge|bundled/i);

delete process.env.MLT_FORCE_BUNDLED_BROWSER;
const preferred = resolveBrowserLaunchTarget();
assert.ok(
  ["system_chrome", "system_edge", "bundled_chromium"].includes(preferred.mode),
  `unexpected mode ${preferred.mode}`,
);
if (preferred.channel) {
  assert.ok(preferred.executable_path, "system browser must have a path");
  assert.ok(
    preferred.channel === "chrome" || preferred.channel === "msedge",
    `unexpected channel ${preferred.channel}`,
  );
}

assert.ok(browserLaunchArgs().includes("--disable-blink-features=AutomationControlled"));
assert.ok(browserIgnoreDefaultArgs().includes("--enable-automation"));
assert.ok(browserIgnoreDefaultArgs().includes("--use-mock-keychain"));
assert.ok(browserIgnoreDefaultArgs().includes("--password-store=basic"));

const msg = bundledBrowserDealerMessage();
assert.match(msg, /Google Chrome/i);
assert.match(msg, /Microsoft Edge/i);

const tmp = fs.mkdtempSync(path.join(os.tmpdir(), "mlt-profile-"));
const configured = path.join(tmp, "browser-profile");
const chromeTarget = {
  mode: "system_chrome",
  channel: "chrome",
  label: "Google Chrome",
  process_name_hint: "Google Chrome",
  executable_path: "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
};
const edgeTarget = {
  mode: "system_edge",
  channel: "msedge",
  label: "Microsoft Edge",
  process_name_hint: "Microsoft Edge",
  executable_path: "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
};
const bundledTarget = {
  mode: "bundled_chromium",
  channel: null,
  label: "bundled",
  process_name_hint: "Chrome for Testing",
  executable_path: null,
};

assert.equal(
  resolvePersistentProfileDir(configured, chromeTarget),
  path.join(tmp, "chrome-profile"),
);
assert.equal(
  resolvePersistentProfileDir(configured, edgeTarget),
  path.join(tmp, "edge-profile"),
);
assert.equal(
  resolvePersistentProfileDir(configured, bundledTarget),
  path.join(tmp, "browser-profile"),
);

// Legacy Chrome session in browser-profile is preserved when chrome-profile is empty.
fs.mkdirSync(path.join(configured, "Default"), { recursive: true });
assert.equal(
  resolvePersistentProfileDir(configured, chromeTarget),
  configured,
);
// Edge never shares the Chrome/legacy profile.
assert.equal(
  resolvePersistentProfileDir(configured, edgeTarget),
  path.join(tmp, "edge-profile"),
);

const all = allPersistentProfileDirs(configured);
assert.ok(all.includes(path.join(tmp, "chrome-profile")));
assert.ok(all.includes(path.join(tmp, "edge-profile")));
assert.ok(all.includes(path.join(tmp, "browser-profile")));

fs.rmSync(tmp, { recursive: true, force: true });

if (prev === undefined) {
  delete process.env.MLT_FORCE_BUNDLED_BROWSER;
} else {
  process.env.MLT_FORCE_BUNDLED_BROWSER = prev;
}

console.log("chrome-channel.test.mjs: ok");
