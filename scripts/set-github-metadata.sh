#!/usr/bin/env bash
set -euo pipefail

# Rename and configure the public SteamTron desktop repository.
# Requires an authenticated GitHub CLI: gh auth login

OWNER="krunaldodiya"
OLD_DESKTOP="$OWNER/SteamTron-Desktop"
NEW_DESKTOP="$OWNER/steamtron-desktop"

if ! command -v gh >/dev/null 2>&1; then
  echo "gh is required: https://cli.github.com/" >&2
  exit 1
fi
if ! gh auth status >/dev/null 2>&1; then
  echo "GitHub CLI is not authenticated. Run: gh auth login" >&2
  exit 1
fi

if gh repo view "$OLD_DESKTOP" >/dev/null 2>&1; then
  gh repo rename steamtron-desktop --repo "$OLD_DESKTOP" --yes
fi

gh repo edit "$NEW_DESKTOP" \
  --visibility public \
  --accept-visibility-change-consequences \
  --description "Open-source Steam library, fixes, and managed-game manager for Linux and Windows" \
  --homepage "https://steamtron.vercel.app" \
  --add-topic steam,steam-library,tauri,rust,linux,windows,protondb,igdb,desktop-app

git remote set-url origin "git@github.com:$NEW_DESKTOP.git"
echo "Desktop origin updated to git@github.com:$NEW_DESKTOP.git"
echo "Verify with: gh repo view $NEW_DESKTOP"
