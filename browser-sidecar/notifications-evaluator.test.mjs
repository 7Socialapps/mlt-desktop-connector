/**
 * Unit tests for notifications-evaluator.mjs (run: node notifications-evaluator.test.mjs)
 */
import assert from "node:assert/strict";
import { evaluateNotificationsFromDetection } from "./notifications-evaluator.mjs";

function testNotificationsReady() {
  const result = evaluateNotificationsFromDetection(
    { state: "facebook_logged_in", reason_code: "nav_present" },
    "https://www.facebook.com/notifications",
  );
  assert.equal(result.status, "notifications_ready");
  assert.equal(result.unread_count, null);
}

function testNotificationsLoginRequired() {
  const result = evaluateNotificationsFromDetection(
    { state: "facebook_logged_out", reason_code: "login_page" },
    "https://www.facebook.com/login.php",
  );
  assert.equal(result.status, "notifications_login_required");
}

function testNotificationsUnavailableWhenDisabled() {
  const result = evaluateNotificationsFromDetection(
    { state: "facebook_disabled_account", reason_code: "account_disabled" },
    "https://www.facebook.com/",
  );
  assert.equal(result.status, "notifications_unavailable");
}

function testNotificationsNotCheckedOffFacebook() {
  const result = evaluateNotificationsFromDetection(
    { state: "facebook_not_checked", reason_code: "not_facebook" },
    "about:blank",
  );
  assert.equal(result.status, "notifications_not_checked");
}

testNotificationsReady();
testNotificationsLoginRequired();
testNotificationsUnavailableWhenDisabled();
testNotificationsNotCheckedOffFacebook();
console.log("notifications-evaluator.test.mjs: all tests passed");
