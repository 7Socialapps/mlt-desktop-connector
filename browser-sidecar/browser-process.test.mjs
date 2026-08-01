/**
 * Regression tests for launchPersistentContext PID/profile behavior.
 */
import assert from "node:assert/strict";
import {
  isBrowserContextConnected,
  profileStateWhileBrowserRunning,
  resolveBrowserProcessPid,
} from "./browser-process.mjs";

function testResolvePidWithStandardBrowser() {
  const ctx = {
    browser() {
      return {
        process() {
          return { pid: 4242 };
        },
      };
    },
  };
  assert.equal(resolveBrowserProcessPid(ctx), 4242);
}

function testResolvePidWhenProcessMissing() {
  const ctx = {
    browser() {
      return {};
    },
  };
  assert.equal(resolveBrowserProcessPid(ctx), null);
}

function testResolvePidWhenProcessNotFunction() {
  const ctx = {
    browser() {
      return { process: "not-a-function" };
    },
  };
  assert.equal(resolveBrowserProcessPid(ctx), null);
}

function testResolvePidWhenBrowserNull() {
  const ctx = {
    browser() {
      return null;
    },
  };
  assert.equal(resolveBrowserProcessPid(ctx), null);
}

function testResolvePidNeverThrows() {
  const ctx = {
    browser() {
      throw new Error("context.browser(...).process is not a function");
    },
  };
  assert.equal(resolveBrowserProcessPid(ctx), null);
}

function testConnectedWithoutBrowserObject() {
  const ctx = {
    browser() {
      return null;
    },
    pages() {
      return [{}];
    },
  };
  assert.equal(isBrowserContextConnected(ctx, "ready"), true);
}

function testNotConnectedWhenStopped() {
  const ctx = { pages() { return []; } };
  assert.equal(isBrowserContextConnected(ctx, "stopped"), false);
}

function testProfileReadyWhileRunning() {
  assert.equal(
    profileStateWhileBrowserRunning("ready", { pages() { return []; } }),
    "profile_ready",
  );
  assert.equal(profileStateWhileBrowserRunning("stopped", {}), null);
}

testResolvePidWithStandardBrowser();
testResolvePidWhenProcessMissing();
testResolvePidWhenProcessNotFunction();
testResolvePidWhenBrowserNull();
testResolvePidNeverThrows();
testConnectedWithoutBrowserObject();
testNotConnectedWhenStopped();
testProfileReadyWhileRunning();
console.log("browser-process.test.mjs: all tests passed");
