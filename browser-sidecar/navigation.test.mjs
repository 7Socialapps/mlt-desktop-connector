/**
 * Unit tests for navigation.mjs destination map (run: node navigation.test.mjs)
 */
import assert from "node:assert/strict";
import { destinationUrl, DESTINATIONS } from "./navigation.mjs";

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

testDestinationUrls();
testUnknownDestination();
console.log("navigation.test.mjs: all tests passed");
