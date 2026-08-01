# Packaging staging (gitignored binaries)

`scripts/prepare-bundle-resources.sh` populates:

- `browser-sidecar/node_modules/{playwright,playwright-core}` — copied from root `npm ci`
- `packaging/node/` — Node LTS binary for the build host arch

These are embedded via `src-tauri/tauri.conf.json` → `bundle.resources`.
