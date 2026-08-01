/**
 * Final verification before ready_for_review — all fields, images, no validation errors.
 * Does NOT click Next or Publish.
 */
import { VEHICLE_CREATE_FIELDS, PHOTO_FIELD } from "./selectors/v1.mjs";
import { FORM_CONTROL_HELPERS } from "./form-controls.mjs";

/**
 * @param {import("playwright").Page} page
 * @param {Record<string, string>} expectedValues
 * @param {number} expectedImageCount
 */
export async function verifyFilledFormFromPage(page, expectedValues, expectedImageCount) {
  const fieldDefs = VEHICLE_CREATE_FIELDS.filter((f) => {
    const val = expectedValues[f.key];
    return val && String(val).trim() !== "" && !f.optional;
  });

  const script = `
    (async () => {
      ${FORM_CONTROL_HELPERS}
      const fieldDefs = ${JSON.stringify(fieldDefs)};
      const expectedValues = ${JSON.stringify(expectedValues)};
      const expectedImageCount = ${expectedImageCount};
      const photoFallbacks = ${JSON.stringify(PHOTO_FIELD.cssFallbacks)};

      const fields_ok = [];
      const fields_missing = [];
      const fields_mismatch = [];

      for (const field of fieldDefs) {
        const expected = String(expectedValues[field.key] ?? "").trim();
        if (!expected) continue;
        const control = resolveControl(field);
        if (!control) {
          fields_missing.push(field.key);
          continue;
        }
        const actual = readControlValue(control);
        const expNorm = expected.toLowerCase();
        const actNorm = actual.toLowerCase();
        if (!actual) {
          fields_missing.push(field.key);
        } else if (actNorm.includes(expNorm) || expNorm.includes(actNorm)) {
          fields_ok.push(field.key);
        } else {
          fields_mismatch.push(field.key);
        }
      }

      const bodyText = (document.body?.innerText ?? "").toLowerCase();
      const hasValidationErrors =
        bodyText.includes("required") &&
        (bodyText.includes("field") || bodyText.includes("enter"));

      const thumbs = document.querySelectorAll(
        'img[src*="blob:"], img[src*="scontent"], [aria-label*="photo" i] img',
      );
      const image_count = thumbs.length;

      const buttons = Array.from(
        document.querySelectorAll('button, [role="button"], div[role="button"]'),
      );
      const buttonText = buttons.map((b) => (b.textContent ?? "").trim().toLowerCase());
      const has_next_button = buttonText.some((t) => t === "next" || t.startsWith("next"));
      const has_publish_button = buttonText.some(
        (t) => t === "publish" || t.includes("publish"),
      );

      const photoInput = q(photoFallbacks);

      let ready =
        fields_missing.length === 0 &&
        fields_mismatch.length === 0 &&
        !hasValidationErrors &&
        (has_next_button || has_publish_button);

      if (expectedImageCount > 0) {
        ready = ready && image_count >= expectedImageCount;
      }

      let reason_code = "form_verified";
      if (fields_missing.length > 0) reason_code = "fields_missing";
      else if (fields_mismatch.length > 0) reason_code = "fields_mismatch";
      else if (hasValidationErrors) reason_code = "validation_errors";
      else if (expectedImageCount > 0 && image_count < expectedImageCount) {
        reason_code = "images_incomplete";
      } else if (!has_next_button && !has_publish_button) {
        reason_code = "no_next_or_publish";
      }

      return {
        ready,
        reason_code,
        fields_ok,
        fields_missing,
        fields_mismatch,
        image_count,
        expected_image_count: expectedImageCount,
        has_validation_errors: hasValidationErrors,
        has_next_button,
        has_publish_button,
        next_or_publish_clicked: false,
        photo_input_present: Boolean(photoInput),
      };
    })();
  `;

  const domReport = await page.evaluate(script);

  return {
    ...domReport,
    checked_at: new Date().toISOString(),
    current_url: page.url(),
  };
}
