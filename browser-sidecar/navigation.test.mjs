/**
 * Unit tests for navigation.mjs destination map (run: node navigation.test.mjs)
 */
import assert from "node:assert/strict";
import { destinationUrl, DESTINATIONS, detectRedirect } from "./navigation.mjs";

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

testDestinationUrls();
testUnknownDestination();
testRedirectDetection();
console.log("navigation.test.mjs: all tests passed");
