/**
 * Unit tests for vehicle-create-verifier.mjs (run: node vehicle-create-verifier.test.mjs)
 */
import assert from "node:assert/strict";
import {
  evaluateVehicleCreateReadiness,
  isVehicleCreateUrl,
} from "./vehicle-create-verifier.mjs";

function readySignals(overrides = {}) {
  return {
    url: "https://www.facebook.com/marketplace/create/vehicle",
    title: "Create Vehicle Listing",
    hasLoginForm: false,
    hasCheckpointText: false,
    hasCreateHeading: true,
    hasFormLandmarks: true,
    hasPhotoUploadArea: true,
    hasVehicleControls: true,
    ...overrides,
  };
}

function testVehicleCreateUrl() {
  assert.equal(
    isVehicleCreateUrl("https://www.facebook.com/marketplace/create/vehicle"),
    true,
  );
  assert.equal(
    isVehicleCreateUrl("https://www.facebook.com/marketplace/"),
    false,
  );
}

function testReadyWhenAllSignalsMet() {
  const result = evaluateVehicleCreateReadiness(readySignals());
  assert.equal(result.ready, true);
  assert.equal(result.reason_code, "vehicle_create_ready");
  assert.ok(result.signals_met.includes("vehicle_create_url"));
  assert.equal(result.signals_missing.length, 0);
}

function testNotReadyOnLogin() {
  const result = evaluateVehicleCreateReadiness(
    readySignals({
      url: "https://www.facebook.com/login.php",
      hasLoginForm: true,
    }),
  );
  assert.equal(result.ready, false);
  assert.equal(result.reason_code, "login_or_checkpoint");
}

function testNotReadyOnCheckpoint() {
  const result = evaluateVehicleCreateReadiness(
    readySignals({
      hasCheckpointText: true,
    }),
  );
  assert.equal(result.ready, false);
  assert.equal(result.reason_code, "login_or_checkpoint");
}

function testMissingPhotoUpload() {
  const result = evaluateVehicleCreateReadiness(
    readySignals({ hasPhotoUploadArea: false }),
  );
  assert.equal(result.ready, false);
  assert.ok(result.signals_missing.includes("photo_upload_area"));
}

function testMissingVehicleControls() {
  const result = evaluateVehicleCreateReadiness(
    readySignals({ hasVehicleControls: false }),
  );
  assert.equal(result.ready, false);
  assert.ok(result.signals_missing.includes("vehicle_controls"));
}

function testWrongUrlNotReady() {
  const result = evaluateVehicleCreateReadiness(
    readySignals({
      url: "https://www.facebook.com/marketplace/",
    }),
  );
  assert.equal(result.ready, false);
  assert.ok(result.signals_missing.includes("vehicle_create_url"));
}

testVehicleCreateUrl();
testReadyWhenAllSignalsMet();
testNotReadyOnLogin();
testNotReadyOnCheckpoint();
testMissingPhotoUpload();
testMissingVehicleControls();
testWrongUrlNotReady();
console.log("vehicle-create-verifier.test.mjs: all tests passed");
