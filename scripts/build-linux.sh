#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
bundles="${STEAMTRON_LINUX_BUNDLES:-deb,rpm,appimage}"
exec npx --yes @tauri-apps/cli@2 build --bundles "$bundles"
