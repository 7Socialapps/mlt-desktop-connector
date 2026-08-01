/**
 * Unit tests for navigation.mjs destination map (run: node navigation.test.mjs)
 */
import assert from "node:assert/strict";
import {
  destinationUrl,
  DESTINATIONS,
  detectRedirect,
  isBlankUrl,
  POST_LAUNCH_NAVIGATION_TARGET,
} from "./navigation.mjs";

function testDestinationUrls() {
  assert.equal(destinationUrl("marketplace"), DESTINATIONS.marketplace);
  assert.equal(
    destinationUrl("marketplace_create_vehicle"),
    "https://www.facebook.com/marketplace/create/vehicle",
  );
  assert.equal(destinationUrl("messenger"), DESTINATIONS.messenger);
  assert.equal(destinationUrl("notifications"), DESTINATIONS.notifications);
  assert.equal(destinationUrl("facebook_home"), DESTINATIONS.facebook_home);
}

function testUnknownDestination() {
  assert.equal(destinationUrl("unknown_place"), null);
}

function testRedirectDetection() {
  assert.equal(
    detectRedirect(
      "https://www.facebook.com/marketplace/",
      "https://www.facebook.com/login.php",
    ),
    true,
  );
  assert.equal(
    detectRedirect(
      "https://www.facebook.com/marketplace/",
      "https://www.facebook.com/marketplace/",
    ),
    false,
  );
  assert.equal(
    detectRedirect(
      "https://www.facebook.com/messages/",
      "https://www.facebook.com/",
    ),
    true,
  );
}

function testBlankUrlDetection() {
  assert.equal(isBlankUrl("about:blank"), true);
  assert.equal(isBlankUrl(""), true);
  assert.equal(isBlankUrl(null), true);
  assert.equal(isBlankUrl(undefined), true);
  assert.equal(isBlankUrl("https://www.facebook.com/"), false);
}

function testPostLaunchNavigationTarget() {
  assert.equal(POST_LAUNCH_NAVIGATION_TARGET, DESTINATIONS.facebook_home);
  assert.equal(
    destinationUrl("facebook_home"),
    "https://www.facebook.com/",
  );
}

testDestinationUrls();
testUnknownDestination();
testRedirectDetection();
testBlankUrlDetection();
testPostLaunchNavigationTarget();
console.log("navigation.test.mjs: all tests passed");
