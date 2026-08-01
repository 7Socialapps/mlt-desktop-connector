#!/usr/bin/env bash
# Prepare Playwright (+ Node) resources for packaged builds.
# Safe to re-run. Does not download Chromium (first-run / explicit install).
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
  ARCH="$(uname -m)"
  case "$OS-$ARCH" in
    Darwin-arm64) NODE_PLATFORM="darwin-arm64" ;;
    Darwin-x86_64) NODE_PLATFORM="darwin-x64" ;;
    Linux-x86_64) NODE_PLATFORM="linux-x64" ;;
    Linux-aarch64) NODE_PLATFORM="linux-arm64" ;;
    *)
      echo "warn: no Node bundle mapping for $OS-$ARCH — packaged app will use system Node" >&2
      NODE_PLATFORM=""
      ;;
  esac
  if [[ -n "$NODE_PLATFORM" ]]; then
    TARBALL="node-${NODE_VER}-${NODE_PLATFORM}.tar.gz"
    URL="https://nodejs.org/dist/${NODE_VER}/${TARBALL}"
    if [[ ! -x "$NODE_STAGE/bin/node" ]]; then
      echo "downloading Node ${NODE_VER} (${NODE_PLATFORM})…"
      TMP="$(mktemp -d)"
      curl -fsSL "$URL" -o "$TMP/$TARBALL"
      tar -xzf "$TMP/$TARBALL" -C "$TMP"
      rm -rf "$NODE_STAGE"
      mkdir -p "$NODE_STAGE"
      # Ship only the node binary (not npm/npx/headers) to keep the DMG small.
      rm -rf "$NODE_STAGE"
      mkdir -p "$NODE_STAGE/bin"
      cp "$TMP/node-${NODE_VER}-${NODE_PLATFORM}/bin/node" "$NODE_STAGE/bin/node"
      chmod +x "$NODE_STAGE/bin/node"
      rm -rf "$TMP"
    fi
    echo "bundled Node → packaging/node ($( "$NODE_STAGE/bin/node" -v ))"
  fi
fi

echo "prepare-bundle-resources: done"
