#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

set -a
if [[ -f .env ]]; then
  # shellcheck disable=SC1091
  source ./.env
fi
set +a

export MLT_ENV="${MLT_ENV:-staging}"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/src-tauri/target}"

echo "Building MLT Desktop Connector (staging) with MLT_ENV=${MLT_ENV}"
npm run build
npm run tauri -- build

echo
echo "Bundle output:"
find src-tauri/target/release/bundle -maxdepth 3 \( -name '*.app' -o -name '*.dmg' \) 2>/dev/null || true
