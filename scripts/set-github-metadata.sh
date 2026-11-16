#!/usr/bin/env bash
set -euo pipefail

# Rename and configure the two SteamTron repositories.
# Requires an authenticated GitHub CLI: gh auth login
#
# The API repository stays private. Only the desktop repository is public.
# Optionally set STEAMTRON_API_DIR to the local checkout of UmuTron-FastAPI so
# its origin can be updated after the rename.

OWNER="krunaldodiya"
OLD_API="$OWNER/UmuTron-FastAPI"
NEW_API="$OWNER/steamtron"
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

repo_exists() {
  gh repo view "$1" >/dev/null 2>&1
}

if repo_exists "$OLD_API"; then
  gh repo rename steamtron --repo "$OLD_API" --yes
fi
gh repo edit "$NEW_API" \
  --visibility private \
  --accept-visibility-change-consequences \
  --description "SteamTron website and API backend (private)" \
  --homepage "https://steamtron.vercel.app" \
  --add-topic steamtron,igdb,fastapi,nextjs,api,turso

if repo_exists "$OLD_DESKTOP"; then
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

if [[ -n "${STEAMTRON_API_DIR:-}" ]]; then
  if [[ -d "$STEAMTRON_API_DIR/.git" ]]; then
    git -C "$STEAMTRON_API_DIR" remote set-url origin "git@github.com:$NEW_API.git"
    echo "API origin updated in $STEAMTRON_API_DIR"
  else
    echo "STEAMTRON_API_DIR is not a git checkout: $STEAMTRON_API_DIR" >&2
  fi
fi

echo "GitHub metadata applied. Verify with:"
echo "  gh repo view $NEW_DESKTOP"
echo "  gh repo view $NEW_API"
