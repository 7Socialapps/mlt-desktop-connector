/**
 * Canonical download / release URLs for the MLT Desktop Connector.
 * Keep in sync with mlt dashboard:
 *   src/lib/webposter/desktopConnectorDownloads.ts
 * and docs/RELEASE-ASSETS.md.
 */

export const DESKTOP_CONNECTOR_GITHUB_REPO = "7Socialapps/mlt-desktop-connector";

export const DESKTOP_CONNECTOR_RELEASES_PAGE =
  `https://github.com/${DESKTOP_CONNECTOR_GITHUB_REPO}/releases`;

export const DESKTOP_CONNECTOR_LATEST_RELEASE_PAGE =
  `https://github.com/${DESKTOP_CONNECTOR_GITHUB_REPO}/releases/latest`;

export const DESKTOP_CONNECTOR_LATEST_API =
  `https://api.github.com/repos/${DESKTOP_CONNECTOR_GITHUB_REPO}/releases/latest`;

/**
 * Once a release ships assets, prefer hardcoding the exact file URL here for
 * offline docs / emails. Leave null until a real binary is published.
 */
export const DESKTOP_CONNECTOR_HARDCODED_DOWNLOADS: {
  mac_arm64: string | null;
  mac_x64: string | null;
  windows_x64: string | null;
} = {
  mac_arm64:
    "https://github.com/7Socialapps/mlt-desktop-connector/releases/download/v1.1.0/MLT.Desktop.Connector_1.1.0_aarch64.dmg",
  mac_x64:
    "https://github.com/7Socialapps/mlt-desktop-connector/releases/download/v1.1.0/MLT.Desktop.Connector_1.1.0_x64.dmg",
  windows_x64: null,
};
