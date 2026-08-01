/**
 * Regression: server.mjs (and form-controls helpers) must parse under Node.
 * A SyntaxError here means Open Facebook can never start the sidecar daemon.
 */
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));

const checks = [
  ["form-controls.mjs", "marketplace/form-controls.mjs"],
  ["form-fill.mjs", "marketplace/form-fill.mjs"],
  ["chrome-channel.mjs", "chrome-channel.mjs"],
  ["server.mjs", "server.mjs"],
];

for (const [label, rel] of checks) {
  const file = path.join(__dirname, rel);
  const result = spawnSync(
    process.execPath,
    ["--check", file],
    { encoding: "utf8" },
  );
  assert.equal(
    result.status,
    0,
    `${label} failed syntax check: ${result.stderr || result.stdout}`,
  );
}

// Runtime import must also succeed (template-literal traps pass --check in some edge cases).
const imported = await import("./marketplace/form-controls.mjs");
assert.equal(typeof imported.FORM_CONTROL_HELPERS, "string");
assert.match(
  imported.FORM_CONTROL_HELPERS,
  /\[\.\*\+\?\^\$\{\}\(\)\|/,
  "injected helpers must contain a real ${} regex character class",
);

console.log("sidecar-import.test.mjs: ok");
