/**
 * Prefer system Chrome/Edge when installed; honor MLT_FORCE_BUNDLED_BROWSER.
 */
import assert from "node:assert/strict";
import {
  browserIgnoreDefaultArgs,
  browserLaunchArgs,
  resolveBrowserLaunchTarget,
} from "./chrome-channel.mjs";

const prev = process.env.MLT_FORCE_BUNDLED_BROWSER;
process.env.MLT_FORCE_BUNDLED_BROWSER = "1";
const forced = resolveBrowserLaunchTarget();
assert.equal(forced.mode, "bundled_chromium");
assert.equal(forced.channel, null);
assert.match(forced.label, /Google Chrome|bundled/i);

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

if (prev === undefined) {
  delete process.env.MLT_FORCE_BUNDLED_BROWSER;
} else {
  process.env.MLT_FORCE_BUNDLED_BROWSER = prev;
}

console.log("chrome-channel.test.mjs: ok");
