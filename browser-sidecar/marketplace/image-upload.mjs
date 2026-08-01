/**
 * Upload vehicle listing images via Playwright file input (M3.2 manifest order).
 */
import { PHOTO_FIELD } from "./selectors/v1.mjs";

const MAX_IMAGES = 20;
const PER_IMAGE_RETRY = 2;
const PROCESSING_BASE_MS = 1500;
const PROCESSING_PER_IMAGE_MS = 400;

/**
 * @typedef {{
 *   index: number,
 *   local_path: string,
 *   source_url?: string,
 * }} ManifestImage
 */

/**
 * @typedef {{
 *   index: number,
 *   ok: boolean,
 *   reason?: string,
 *   attempts?: number,
 * }} ImageUploadResult
 */

/**
 * @param {import("playwright").Page} page
 * @param {ManifestImage[]} images
 * @returns {Promise<{ uploaded: ImageUploadResult[], thumbnail_count: number, expected_count: number, primary_preserved: boolean }>}
 */
export async function uploadVehicleImagesFromPage(page, images) {
  const ordered = [...images]
    .sort((a, b) => a.index - b.index)
    .slice(0, MAX_IMAGES);

  if (ordered.length === 0) {
    return {
      uploaded: [],
      thumbnail_count: 0,
      expected_count: 0,
      primary_preserved: true,
    };
  }

  const selector = PHOTO_FIELD.cssFallbacks.join(", ");
  const fileInput = page.locator(selector).first();
  await fileInput.waitFor({ state: "attached", timeout: 15_000 }).catch(() => {});

  /** @type {ImageUploadResult[]} */
  const uploaded = [];
  const paths = ordered.map((img) => img.local_path);

  for (let attempt = 1; attempt <= PER_IMAGE_RETRY; attempt++) {
    try {
      await fileInput.setInputFiles(paths);
      break;
    } catch (err) {
      if (attempt >= PER_IMAGE_RETRY) {
        for (const img of ordered) {
          uploaded.push({
            index: img.index,
            ok: false,
            reason: err instanceof Error ? err.message : "setInputFiles_failed",
            attempts: attempt,
          });
        }
        return {
          uploaded,
          thumbnail_count: 0,
          expected_count: ordered.length,
          primary_preserved: false,
        };
      }
      await page.waitForTimeout(800 * attempt);
    }
  }

  const waitMs = PROCESSING_BASE_MS + PROCESSING_PER_IMAGE_MS * ordered.length;
  await page.waitForTimeout(Math.min(waitMs, 30_000));

  const thumbCount = await countPhotoThumbnails(page);

  for (const img of ordered) {
    uploaded.push({
      index: img.index,
      ok: thumbCount >= img.index + 1,
      reason: thumbCount >= img.index + 1 ? undefined : "thumbnail_missing",
      attempts: 1,
    });
  }

  return {
    uploaded,
    thumbnail_count: thumbCount,
    expected_count: ordered.length,
    primary_preserved: thumbCount >= 1,
  };
}

/**
 * @param {import("playwright").Page} page
 */
async function countPhotoThumbnails(page) {
  return page.evaluate(() => {
    const thumbs = document.querySelectorAll(
      'img[src*="blob:"], img[src*="scontent"], [aria-label*="photo" i] img, [data-testid*="photo"] img',
    );
    return thumbs.length;
  });
}

/**
 * @param {import("playwright").Page} page
 * @param {string} localPath
 */
export async function retrySingleImageUpload(page, localPath) {
  const selector = PHOTO_FIELD.cssFallbacks.join(", ");
  const fileInput = page.locator(selector).first();
  await fileInput.setInputFiles([localPath]);
  await page.waitForTimeout(PROCESSING_BASE_MS);
  const thumbCount = await countPhotoThumbnails(page);
  return { ok: thumbCount > 0 };
}
