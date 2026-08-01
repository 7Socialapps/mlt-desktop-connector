#!/usr/bin/env bash
# Prepare Playwright (+ Node + Chromium) resources for packaged builds.
# Safe to re-run. Chromium is installed at build time into packaging/ms-playwright
# so dealers do not depend on a first-run download for Open Facebook.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

SIDECAR_NM="$ROOT/browser-sidecar/node_modules"
mkdir -p "$SIDECAR_NM"

copy_pkg() {
  local name="$1"
  local src="$ROOT/node_modules/$name"
  local dest="$SIDECAR_NM/$name"
  if [[ ! -d "$src" ]]; then
    echo "error: missing $src — run npm ci first" >&2
    exit 1
  fi
  rm -rf "$dest"
  # Prefer hardlink-friendly copy; fall back to recursive copy.
  if command -v rsync >/dev/null 2>&1; then
    rsync -a --delete "$src/" "$dest/"
  else
    cp -R "$src" "$dest"
  fi
  echo "bundled $name → browser-sidecar/node_modules/$name"
}

copy_pkg playwright
copy_pkg playwright-core

# Optional bundled Node (macOS / Windows). Skip if MLT_SKIP_NODE_BUNDLE=1.
NODE_STAGE="$ROOT/packaging/node"
if [[ "${MLT_SKIP_NODE_BUNDLE:-0}" != "1" ]]; then
  NODE_VER="${MLT_BUNDLE_NODE_VERSION:-v22.16.0}"
  mkdir -p "$NODE_STAGE"
  OS="$(uname -s)"
  # Prefer explicit build target (cross-compile) over host arch.
  TARGET="${CARGO_BUILD_TARGET:-${TAURI_ENV_TARGET_TRIPLE:-}}"
  case "$TARGET" in
    aarch64-apple-darwin) ARCH_HINT="arm64"; NODE_PLATFORM="darwin-arm64"; PW_HOST="mac14-arm64" ;;
    x86_64-apple-darwin) ARCH_HINT="x86_64"; NODE_PLATFORM="darwin-x64"; PW_HOST="mac14" ;;
    x86_64-pc-windows-msvc|x86_64-pc-windows-gnu) ARCH_HINT="x86_64"; NODE_PLATFORM=""; PW_HOST="win64" ;;
    *)
      ARCH="$(uname -m)"
      ARCH_HINT="$ARCH"
      case "$OS-$ARCH" in
        Darwin-arm64) NODE_PLATFORM="darwin-arm64"; PW_HOST="mac14-arm64" ;;
        Darwin-x86_64) NODE_PLATFORM="darwin-x64"; PW_HOST="mac14" ;;
        Linux-x86_64) NODE_PLATFORM="linux-x64"; PW_HOST="" ;;
        Linux-aarch64) NODE_PLATFORM="linux-arm64"; PW_HOST="" ;;
        *)
          echo "warn: no Node bundle mapping for $OS-$ARCH — packaged app will use system Node" >&2
          NODE_PLATFORM=""
          PW_HOST=""
          ;;
      esac
      ;;
  esac

  # Prefer pre-staged arch-specific Node trees when present (CI / dual Mac builds).
  if [[ "$NODE_PLATFORM" == "darwin-arm64" && -x "$ROOT/packaging/node-arm64/bin/node" ]]; then
    rm -rf "$NODE_STAGE"
    mkdir -p "$NODE_STAGE/bin"
    cp "$ROOT/packaging/node-arm64/bin/node" "$NODE_STAGE/bin/node"
    chmod +x "$NODE_STAGE/bin/node"
    echo "bundled Node (node-arm64) → packaging/node ($( "$NODE_STAGE/bin/node" -v ))"
  elif [[ "$NODE_PLATFORM" == "darwin-x64" && -x "$ROOT/packaging/node-x64/bin/node" ]]; then
    rm -rf "$NODE_STAGE"
    mkdir -p "$NODE_STAGE/bin"
    cp "$ROOT/packaging/node-x64/bin/node" "$NODE_STAGE/bin/node"
    chmod +x "$NODE_STAGE/bin/node"
    echo "bundled Node (node-x64) → packaging/node ($( "$NODE_STAGE/bin/node" -v ))"
  elif [[ -n "$NODE_PLATFORM" ]]; then
    TARBALL="node-${NODE_VER}-${NODE_PLATFORM}.tar.gz"
    URL="https://nodejs.org/dist/${NODE_VER}/${TARBALL}"
    if [[ ! -x "$NODE_STAGE/bin/node" ]]; then
      echo "downloading Node ${NODE_VER} (${NODE_PLATFORM})…"
      TMP="$(mktemp -d)"
      curl -fsSL "$URL" -o "$TMP/$TARBALL"
      tar -xzf "$TMP/$TARBALL" -C "$TMP"
      rm -rf "$NODE_STAGE"
      mkdir -p "$NODE_STAGE/bin"
      cp "$TMP/node-${NODE_VER}-${NODE_PLATFORM}/bin/node" "$NODE_STAGE/bin/node"
      chmod +x "$NODE_STAGE/bin/node"
      rm -rf "$TMP"
    fi
    echo "bundled Node → packaging/node ($( "$NODE_STAGE/bin/node" -v ))"
  fi
fi

# Bundle Chromium for the target arch (required for reliable Open Facebook).
# Skip with MLT_SKIP_CHROMIUM_BUNDLE=1 for faster local iteration.
MS_PLAYWRIGHT="$ROOT/packaging/ms-playwright"
if [[ "${MLT_SKIP_CHROMIUM_BUNDLE:-0}" != "1" ]]; then
  mkdir -p "$MS_PLAYWRIGHT"
  export PLAYWRIGHT_BROWSERS_PATH="$MS_PLAYWRIGHT"
  if [[ -n "${PW_HOST:-}" ]]; then
    export PLAYWRIGHT_HOST_PLATFORM_OVERRIDE="$PW_HOST"
    echo "installing Chromium for PLAYWRIGHT_HOST_PLATFORM_OVERRIDE=${PW_HOST}…"
  else
    unset PLAYWRIGHT_HOST_PLATFORM_OVERRIDE || true
    echo "installing Chromium for host platform…"
  fi
  rm -rf "$MS_PLAYWRIGHT/__dirlock"
  npx playwright install chromium
  # Headed Open Facebook only — drop headless shell to shrink the DMG.
  rm -rf "$MS_PLAYWRIGHT"/chromium_headless_shell-*
  # Clear quarantine so Gatekeeper does not block the nested browser after DMG install.
  if [[ "$(uname -s)" == "Darwin" ]]; then
    xattr -cr "$MS_PLAYWRIGHT" 2>/dev/null || true
  fi
  # Sanity: executable must exist for this install.
  DETECT_JSON="$(
    PLAYWRIGHT_BROWSERS_PATH="$MS_PLAYWRIGHT" \
      ${PW_HOST:+PLAYWRIGHT_HOST_PLATFORM_OVERRIDE="$PW_HOST"} \
      node "$ROOT/browser-sidecar/cli.mjs" detect
  )"
  echo "chromium detect: $DETECT_JSON"
  if ! echo "$DETECT_JSON" | grep -q '"chromium_installed":true'; then
    echo "error: Chromium bundle incomplete — detect did not report installed" >&2
    exit 1
  fi
  echo "bundled Chromium → packaging/ms-playwright ($(du -sh "$MS_PLAYWRIGHT" | awk '{print $1}'))"
else
  echo "warn: MLT_SKIP_CHROMIUM_BUNDLE=1 — packaged Open Facebook will need first-run download" >&2
fi

echo "prepare-bundle-resources: done (arch_hint=${ARCH_HINT:-host})"
