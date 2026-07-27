#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

set -a
if [[ -f .env.production ]]; then
  # shellcheck disable=SC1091
  source ./.env.production
elif [[ -f .env ]]; then
  # shellcheck disable=SC1091
  source ./.env
fi
set +a

export MLT_ENV=production
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/src-tauri/target}"

echo "Building MLT Desktop Connector (production) with MLT_ENV=${MLT_ENV}"
npm run build
npm run tauri -- build

echo
echo "Bundle output:"
find src-tauri/target/release/bundle -maxdepth 3 \( -name '*.app' -o -name '*.dmg' \) 2>/dev/null || true
