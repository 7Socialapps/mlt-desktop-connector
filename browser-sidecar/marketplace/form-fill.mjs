/**
 * Fill Facebook Marketplace vehicle create form from payload values.
 */
import { SELECTOR_VERSION, VEHICLE_CREATE_FIELDS } from "./selectors/v1.mjs";
import { FORM_CONTROL_HELPERS } from "./form-controls.mjs";

/**
 * @param {import("playwright").Page} page
 * @param {Record<string, string>} payload
 */
export async function fillVehicleFormFromPage(page, payload) {
  const fields = VEHICLE_CREATE_FIELDS.map((f) => ({
    ...f,
    value: payload[f.key] ?? "",
  }));

  const script = `
    (async () => {
      ${FORM_CONTROL_HELPERS}
      const fieldDefs = ${JSON.stringify(fields)};
      const selectorVersion = ${JSON.stringify(SELECTOR_VERSION)};

      const results = [];

      async function fillField(field) {
        const value = field.value;
        if (!value || String(value).trim() === "") {
          return {
            field: field.key,
            ok: true,
            reason: "empty_skipped",
            optional: Boolean(field.optional),
          };
        }

        const control = resolveControl(field);
        if (!control) {
          return {
            field: field.key,
            ok: false,
            expected: String(value),
            reason: "control_not_found",
            optional: Boolean(field.optional),
          };
        }

        let actual = "";
        try {
          if (field.control === "combobox") {
            const picked = await pickComboboxOption(control, value);
            if (!picked) {
              actual = await fillTextControl(control, value);
            } else {
              actual = readControlValue(control);
            }
          } else if (field.control === "textarea" || field.control === "text" || field.control === "location") {
            let normalized = String(value);
            if (field.key === "price") normalized = normalizePrice(value);
            if (field.key === "mileage") normalized = normalizeMileage(value);
            actual = await fillTextControl(control, normalized);
          } else {
            actual = await fillTextControl(control, value);
          }
        } catch (err) {
          return {
            field: field.key,
            ok: false,
            expected: String(value),
            reason: err instanceof Error ? err.message : "fill_error",
            optional: Boolean(field.optional),
          };
        }

        const expectedNorm = String(value).trim().toLowerCase();
        const actualNorm = String(actual).trim().toLowerCase();
        const ok =
          actualNorm.includes(expectedNorm) ||
          expectedNorm.includes(actualNorm) ||
          actualNorm.length > 0;

        return {
          field: field.key,
          ok,
          expected: String(value),
          actual,
          reason: ok ? undefined : "readback_mismatch",
          optional: Boolean(field.optional),
        };
      }

      const order = [
        "listing_type", "category", "year", "make", "model", "trim",
        "price", "mileage", "body_style", "condition", "exterior_color",
        "interior_color", "transmission", "drivetrain", "fuel_type",
        "title", "description", "location",
      ];

      const byKey = Object.fromEntries(fieldDefs.map((f) => [f.key, f]));
      for (const key of order) {
        const field = byKey[key];
        if (!field) continue;
        const result = await fillField(field);
        results.push(result);
        if (["year", "make", "model", "category"].includes(key)) {
          await new Promise((r) => setTimeout(r, 400));
        }
      }

      const filled = results.filter((r) => r.ok && r.reason !== "empty_skipped").map((r) => r.field);
      const skipped = results.filter((r) => r.reason === "empty_skipped").map((r) => r.field);
      const failed = results.filter((r) => !r.ok && !r.optional).map((r) => r.field);

      return {
        selector_version: selectorVersion,
        fields: results,
        filled,
        failed,
        skipped,
      };
    })();
  `;

  return page.evaluate(script);
}
