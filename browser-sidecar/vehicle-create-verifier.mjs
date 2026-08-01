/**
 * Vehicle create form readiness — multi-signal verification (M3.3).
 * Pure evaluation for unit tests; page probe for sidecar RPC.
 */
import { isMarketplaceUrl } from "./facebook-detector.mjs";

const VEHICLE_CREATE_PATH = "/marketplace/create/vehicle";

/**
 * @param {string} url
 * @returns {boolean}
 */
export function isVehicleCreateUrl(url) {
  try {
    const parsed = new URL(url);
    return parsed.pathname.includes(VEHICLE_CREATE_PATH);
  } catch {
    return false;
  }
}

/**
 * @typedef {{
 *   url: string,
 *   title: string,
 *   hasLoginForm: boolean,
 *   hasCheckpointText: boolean,
 *   hasCreateHeading: boolean,
 *   hasFormLandmarks: boolean,
 *   hasPhotoUploadArea: boolean,
 *   hasVehicleControls: boolean,
 * }} VehicleCreateSignals
 */

/**
 * Multi-signal readiness evaluation.
 * @param {VehicleCreateSignals} signals
 * @returns {{ ready: boolean, reason_code: string, signals_met: string[], signals_missing: string[] }}
 */
export function evaluateVehicleCreateReadiness(signals) {
  const {
    url,
    title,
    hasLoginForm,
    hasCheckpointText,
    hasCreateHeading,
    hasFormLandmarks,
    hasPhotoUploadArea,
    hasVehicleControls,
  } = signals;

  const lowerUrl = (url || "").toLowerCase();
  const lowerTitle = (title || "").toLowerCase();

  if (
    lowerUrl.includes("/login") ||
    lowerUrl.includes("/checkpoint") ||
    hasLoginForm ||
    hasCheckpointText
  ) {
    return {
      ready: false,
      reason_code: "login_or_checkpoint",
      signals_met: [],
      signals_missing: ["authenticated_session"],
    };
  }

  const checks = {
    vehicle_create_url: isVehicleCreateUrl(url),
    marketplace_context: isMarketplaceUrl(url),
    create_heading:
      hasCreateHeading ||
      lowerTitle.includes("vehicle") ||
      lowerTitle.includes("create listing") ||
      lowerTitle.includes("sell"),
    form_landmarks: hasFormLandmarks,
    photo_upload_area: hasPhotoUploadArea,
    vehicle_controls: hasVehicleControls,
  };

  const signals_met = Object.entries(checks)
    .filter(([, ok]) => ok)
    .map(([name]) => name);
  const signals_missing = Object.entries(checks)
    .filter(([, ok]) => !ok)
    .map(([name]) => name);

  const ready =
    checks.vehicle_create_url &&
    checks.create_heading &&
    checks.form_landmarks &&
    checks.photo_upload_area &&
    checks.vehicle_controls;

  return {
    ready,
    reason_code: ready ? "vehicle_create_ready" : signals_missing[0] ?? "not_ready",
    signals_met,
    signals_missing,
  };
}

/**
 * Collect signals from a live Playwright page.
 * @param {import("playwright").Page} page
 * @returns {Promise<VehicleCreateSignals & { current_url: string }>}
 */
export async function collectVehicleCreateSignals(page) {
  const url = page.url();
  const title = await page.title().catch(() => "");

  const dom = await page.evaluate(() => {
    const bodyText = (document.body?.innerText ?? "").toLowerCase();
    const hasLoginForm = Boolean(
      document.querySelector('input[name="email"], input[name="pass"], form[action*="login"]'),
    );
    const hasCheckpointText =
      bodyText.includes("checkpoint") ||
      bodyText.includes("confirm your identity") ||
      bodyText.includes("security check");

    const headings = Array.from(document.querySelectorAll("h1, h2, h3, [role=heading]"));
    const headingText = headings.map((h) => (h.textContent ?? "").toLowerCase()).join(" ");
    const hasCreateHeading =
      headingText.includes("vehicle") ||
      headingText.includes("create") ||
      headingText.includes("sell");

    const hasFormLandmarks = Boolean(
      document.querySelector("form") ||
        document.querySelector("[role=form]") ||
        document.querySelector("label") ||
        document.querySelector("[aria-label]"),
    );

    const hasPhotoUploadArea = Boolean(
      document.querySelector('input[type="file"]') ||
        bodyText.includes("add photos") ||
        bodyText.includes("upload photos") ||
        document.querySelector("[aria-label*='photo' i]"),
    );

    const hasVehicleControls = Boolean(
      document.querySelector("[aria-label*='year' i]") ||
        document.querySelector("[aria-label*='make' i]") ||
        document.querySelector("[aria-label*='model' i]") ||
        bodyText.includes("year") ||
        bodyText.includes("make") ||
        bodyText.includes("model") ||
        document.querySelector("select") ||
        document.querySelector("[role=combobox]"),
    );

    return {
      hasLoginForm,
      hasCheckpointText,
      hasCreateHeading,
      hasFormLandmarks,
      hasPhotoUploadArea,
      hasVehicleControls,
    };
  });

  return { url, title, ...dom, current_url: url };
}

/**
 * @param {import("playwright").Page} page
 */
export async function verifyVehicleCreateFromPage(page) {
  const signals = await collectVehicleCreateSignals(page);
  const evaluation = evaluateVehicleCreateReadiness(signals);
  return {
    ...evaluation,
    current_url: signals.current_url,
    page_title: signals.title,
    checked_at: new Date().toISOString(),
  };
}
