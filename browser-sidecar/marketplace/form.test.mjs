import { SELECTOR_VERSION, VEHICLE_CREATE_FIELDS, fieldByKey } from "./selectors/v1.mjs";
import { evaluateVehicleCreateReadiness } from "../vehicle-create-verifier.mjs";

function assert(cond, msg) {
  if (!cond) throw new Error(msg);
}

assert(SELECTOR_VERSION === "1", "selector version is 1");
assert(VEHICLE_CREATE_FIELDS.length >= 15, "registry covers required fields");
assert(fieldByKey("year")?.control === "combobox", "year is combobox");
assert(fieldByKey("description")?.control === "textarea", "description is textarea");

const requiredKeys = [
  "category", "year", "make", "model", "price", "condition", "description",
];
for (const key of requiredKeys) {
  assert(fieldByKey(key), `missing field ${key}`);
}

const ready = evaluateVehicleCreateReadiness({
  url: "https://www.facebook.com/marketplace/create/vehicle",
  title: "Create Vehicle Listing",
  hasLoginForm: false,
  hasCheckpointText: false,
  hasCreateHeading: true,
  hasFormLandmarks: true,
  hasPhotoUploadArea: true,
  hasVehicleControls: true,
});
assert(ready.ready, "readiness evaluator accepts complete signals");

console.log("marketplace-form.test.mjs: all tests passed");
