/**
 * Unit tests for marketplace-evaluator.mjs (run via npm test)
 */
import assert from "node:assert/strict";
import { evaluateMarketplaceFromDetection } from "./marketplace-evaluator.mjs";

function testMarketplaceReady() {
  const result = evaluateMarketplaceFromDetection(
    { state: "facebook_logged_in", reason_code: "nav_present" },
    "https://www.facebook.com/marketplace/",
  );
  assert.equal(result.status, "marketplace_ready");
  assert.equal(result.reason_code, "marketplace_loaded");
}

function testLoginRequiredWhenLoggedOut() {
  const result = evaluateMarketplaceFromDetection(
    { state: "facebook_logged_out", reason_code: "login_page" },
    "https://www.facebook.com/login.php",
  );
  assert.equal(result.status, "marketplace_login_required");
}

function testCheckpointBlocksMarketplace() {
  const result = evaluateMarketplaceFromDetection(
    { state: "facebook_checkpoint", reason_code: "checkpoint_url" },
    "https://www.facebook.com/checkpoint/",
  );
  assert.equal(result.status, "marketplace_checkpoint");
}

function testLoggedInButNotMarketplaceUrl() {
  const result = evaluateMarketplaceFromDetection(
    { state: "facebook_logged_in", reason_code: "nav_present" },
    "https://www.facebook.com/",
  );
  assert.equal(result.status, "marketplace_unavailable");
  assert.equal(result.reason_code, "not_marketplace_url");
}

function testMfaRequiresLogin() {
  const result = evaluateMarketplaceFromDetection(
    { state: "facebook_mfa_required", reason_code: "mfa_prompt" },
    "https://www.facebook.com/two_step_verification/",
  );
  assert.equal(result.status, "marketplace_login_required");
}

testMarketplaceReady();
testLoginRequiredWhenLoggedOut();
testCheckpointBlocksMarketplace();
testLoggedInButNotMarketplaceUrl();
testMfaRequiresLogin();
console.log("marketplace-evaluator tests passed");
