/**
 * Unit tests for messenger-evaluator.mjs (run: node messenger-evaluator.test.mjs)
 */
import assert from "node:assert/strict";
import { evaluateMessengerFromDetection } from "./messenger-evaluator.mjs";

function testMessengerReady() {
  const result = evaluateMessengerFromDetection(
    { state: "facebook_logged_in", reason_code: "nav_present" },
    "https://www.facebook.com/messages/t/123",
  );
  assert.equal(result.status, "messenger_ready");
}

function testMessengerLoginRequired() {
  const result = evaluateMessengerFromDetection(
    { state: "facebook_logged_out", reason_code: "login_page" },
    "https://www.facebook.com/login.php",
  );
  assert.equal(result.status, "messenger_login_required");
}

function testMessengerCheckpoint() {
  const result = evaluateMessengerFromDetection(
    { state: "facebook_checkpoint", reason_code: "checkpoint_url" },
    "https://www.facebook.com/checkpoint/",
  );
  assert.equal(result.status, "messenger_checkpoint");
}

function testMessengerUnavailableWhenRestricted() {
  const result = evaluateMessengerFromDetection(
    { state: "facebook_temporary_restriction", reason_code: "temporary_restriction" },
    "https://www.facebook.com/",
  );
  assert.equal(result.status, "messenger_unavailable");
}

function testMessengerNotCheckedOffFacebook() {
  const result = evaluateMessengerFromDetection(
    { state: "facebook_not_checked", reason_code: "not_facebook" },
    "https://example.com/",
  );
  assert.equal(result.status, "messenger_not_checked");
}

testMessengerReady();
testMessengerLoginRequired();
testMessengerCheckpoint();
testMessengerUnavailableWhenRestricted();
testMessengerNotCheckedOffFacebook();
console.log("messenger-evaluator.test.mjs: all tests passed");
