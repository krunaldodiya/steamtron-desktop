#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
exec npx --yes @tauri-apps/cli@2 build --bundles deb,appimage
