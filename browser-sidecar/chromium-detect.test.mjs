/**
 * Regression: detect must require the Chromium binary on disk.
 * executablePath() alone is not enough (returns expected path when missing).
 */
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const cli = path.join(__dirname, "cli.mjs");
const emptyBrowsers = fs.mkdtempSync(path.join(os.tmpdir(), "mlt-pw-empty-"));

const result = spawnSync(process.execPath, [cli, "detect"], {
  cwd: __dirname,
  env: { ...process.env, PLAYWRIGHT_BROWSERS_PATH: emptyBrowsers },
  encoding: "utf8",
});

assert.equal(result.status, 0, `detect exited ${result.status}: ${result.stderr}`);
const line = result.stdout.trim().split("\n").pop();
const json = JSON.parse(line);
assert.equal(json.playwright_installed, true);
assert.equal(
  json.chromium_installed,
  false,
  `expected chromium_installed=false when browsers path is empty, got ${line}`,
);
assert.ok(
  json.detect_error || json.chromium_path,
  "detect should report missing path or detect_error",
);

fs.rmSync(emptyBrowsers, { recursive: true, force: true });
console.log("chromium-detect.test.mjs: ok");
