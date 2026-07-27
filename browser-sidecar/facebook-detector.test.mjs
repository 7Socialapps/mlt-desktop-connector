/**
 * Unit tests for facebook-detector.mjs (run: node facebook-detector.test.mjs)
 */
import assert from "node:assert/strict";
import {
  detectFacebookSession,
  isFacebookUrl,
  isMarketplaceUrl,
} from "./facebook-detector.mjs";

function testLoggedOutLoginPage() {
  const result = detectFacebookSession({
    url: "https://www.facebook.com/login.php",
    title: "Log in to Facebook",
    hasLoginForm: true,
    hasCheckpointText: false,
    hasMfaText: false,
    hasNavBar: false,
    hasLogoutSignal: false,
  });
  assert.equal(result.state, "facebook_logged_out");
  assert.equal(result.reason_code, "login_page");
  assert.equal(result.marketplace_accessible, false);
}

function testLoggedInHome() {
  const result = detectFacebookSession({
    url: "https://www.facebook.com/",
    title: "Facebook",
    hasLoginForm: false,
    hasCheckpointText: false,
    hasMfaText: false,
    hasNavBar: true,
    hasLogoutSignal: true,
  });
  assert.equal(result.state, "facebook_logged_in");
  assert.equal(result.marketplace_accessible, true);
}

function testCheckpoint() {
  const result = detectFacebookSession({
    url: "https://www.facebook.com/checkpoint/",
    title: "Security Check",
    hasLoginForm: false,
    hasCheckpointText: true,
    hasMfaText: false,
    hasNavBar: false,
    hasLogoutSignal: false,
  });
  assert.equal(result.state, "facebook_checkpoint");
}

function testMfa() {
  const result = detectFacebookSession({
    url: "https://www.facebook.com/two_step_verification/",
    title: "Two-Factor Authentication",
    hasLoginForm: false,
    hasCheckpointText: false,
    hasMfaText: true,
    hasNavBar: false,
    hasLogoutSignal: false,
  });
  assert.equal(result.state, "facebook_mfa_required");
}

function testUrlHelpers() {
  assert.equal(isFacebookUrl("https://www.facebook.com/"), true);
  assert.equal(isFacebookUrl("https://example.com/"), false);
  assert.equal(
    isMarketplaceUrl("https://www.facebook.com/marketplace/"),
    true,
  );
}

function testSessionExpired() {
  const result = detectFacebookSession({
    url: "https://www.facebook.com/",
    title: "Session Expired",
    hasLoginForm: false,
    hasCheckpointText: false,
    hasMfaText: false,
    hasNavBar: false,
    hasLogoutSignal: false,
  });
  assert.equal(result.state, "facebook_session_expired");
  assert.equal(result.reason_code, "session_expired");
}

function testLoginInProgress() {
  const result = detectFacebookSession({
    url: "https://www.facebook.com/login.php?login_attempt=1",
    title: "Facebook",
    hasLoginForm: true,
    hasCheckpointText: false,
    hasMfaText: false,
    hasNavBar: false,
    hasLogoutSignal: false,
  });
  assert.equal(result.state, "facebook_login_in_progress");
}

function testNotFacebookUrl() {
  const result = detectFacebookSession({
    url: "https://example.com/",
    title: "Example",
    hasLoginForm: false,
    hasCheckpointText: false,
    hasMfaText: false,
    hasNavBar: false,
    hasLogoutSignal: false,
  });
  assert.equal(result.state, "facebook_not_checked");
  assert.equal(result.reason_code, "not_facebook");
}

testLoggedOutLoginPage();
testLoggedInHome();
testCheckpoint();
testMfa();
testSessionExpired();
testLoginInProgress();
testNotFacebookUrl();
testUrlHelpers();
console.log("facebook-detector tests passed");
