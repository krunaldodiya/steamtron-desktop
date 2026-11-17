#!/usr/bin/env bash
# SteamTron Linux backend repair.
#
# Installs only the x86_64 SLSsteam Moon runtime (library + wrapper). It does
# not install Lumen or the LuaTools plugin, and it does not bundle any binary in
# the SteamTron application. The release is downloaded at repair time and
# installed into the user's global data directory.
#
# This script is embedded in the SteamTron binary as text and written to
# ~/.local/share/steamtron/repair-slsteam.sh before it runs.

set -euo pipefail

REPO="ogb4n/slsteam-moon-x64"
API_URL="https://api.github.com/repos/${REPO}/releases/latest"
SLS_DIR="${HOME}/.local/share/SLSsteam"
SLS_SO="${SLS_DIR}/SLSsteam.so"
SLS_INJECT="${SLS_DIR}/library-inject.so"
SLS_WRAPPER="${SLS_DIR}/path/steam"
SYSTEM_STEAM=/usr/bin/steam
TMP=""

cleanup() {
  if [[ -n "${TMP}" && -d "${TMP}" ]]; then
    rm -rf "${TMP}"
  fi
  return 0
}
trap cleanup EXIT

die() {
  printf 'error: %s\n' "$*" >&2
  exit 1
}

info() {
  printf '\n==> %s\n' "$*"
}

is_elf64() {
  [[ -f "$1" ]] && file -b "$1" 2>/dev/null | grep -q 'ELF 64-bit'
}

require() {
  command -v "$1" >/dev/null 2>&1 || die "required command not found: $1"
}

find_steam_root() {
  local candidate
  for candidate in \
    "${HOME}/.local/share/Steam" \
    "${HOME}/.steam/steam" \
    "${HOME}/.steam/root" \
    "${HOME}/.var/app/com.valvesoftware.Steam/.local/share/Steam"; do
    if [[ -x "${candidate}/steamrt64/steam" ]]; then
      readlink -f "${candidate}"
      return 0
    fi
  done
  return 1
}

require file
require curl
require sha256sum
require unzip
require pgrep
require sudo

STEAM_ROOT="$(find_steam_root)" || die \
  "64-bit Steam (steamrt64) was not found. Start Steam once, then retry."

if is_elf64 "${SLS_SO}" && is_elf64 "${SLS_INJECT}" && [[ -x "${SLS_WRAPPER}" ]]; then
  printf 'SLSsteam Moon is already installed for the 64-bit Steam client.\n'
  printf 'No download was needed.\n'
  exit 0
fi

info "Resolving the latest x86_64 SLSsteam Moon release"
RELEASE_JSON="$(curl -fsSL -H 'User-Agent: steamtron-desktop' "${API_URL}")" \
  || die "could not query the SLSsteam Moon release metadata"
ASSET_URL="$(printf '%s' "${RELEASE_JSON}" \
  | grep -o '"browser_download_url"[[:space:]]*:[[:space:]]*"[^"]*x86_64\.zip"' \
  | head -n1 \
  | sed -E 's/.*"([^"]+)"$/\1/')"
[[ -n "${ASSET_URL}" ]] || die "no x86_64 release zip was found in the latest release"
printf 'Downloading %s\n' "${ASSET_URL}"

TMP="$(mktemp -d)"
ZIP="${TMP}/slsteam-moon-x86_64.zip"
SHA_FILE="${ZIP}.sha256"
curl -fsSL "${ASSET_URL}" -o "${ZIP}" || die "download failed"
curl -fsSL "${ASSET_URL}.sha256" -o "${SHA_FILE}" || die "checksum download failed"
EXPECTED="$(awk 'NR==1 { print $1 }' "${SHA_FILE}")"
ACTUAL="$(sha256sum "${ZIP}" | awk '{ print $1 }')"
[[ -n "${EXPECTED}" && "${EXPECTED}" == "${ACTUAL}" ]] \
  || die "SLSsteam Moon archive checksum did not match"

info "Extracting the release"
unzip -q "${ZIP}" -d "${TMP}/release" || die "could not extract the release"
SETUP="$(find "${TMP}/release" -type f -name setup.sh -print -quit)"
[[ -n "${SETUP}" ]] || die "setup.sh was not present in the release"

# Preserve the stock system launcher before the uninstaller removes the
# user-level backup. The installer recreates the shim afterwards.
STOCK_STEAM=""
if [[ -f "${SYSTEM_STEAM}" ]] \
  && grep -q 'slsteam-moon system launcher shim' "${SYSTEM_STEAM}" 2>/dev/null; then
  BACKUP="${SLS_DIR}/system-launcher-backup/usr/bin/steam.orig"
  if [[ -f "${BACKUP}" ]]; then
    STOCK_STEAM="${TMP}/steam.stock"
    cp -a "${BACKUP}" "${STOCK_STEAM}"
  else
    printf 'warning: /usr/bin/steam is patched but its stock backup is missing\n' >&2
  fi
fi

info "Stopping Steam"
pkill -TERM -x steam 2>/dev/null || true
pkill -TERM -x steamwebhelper 2>/dev/null || true
sleep 2
pkill -KILL -x steam 2>/dev/null || true
pkill -KILL -x steamwebhelper 2>/dev/null || true

info "Removing the old SLSsteam Moon installation"
# setup.sh is the release's own uninstaller. It is expected to fail when there
# was no previous installation; that is not a repair failure.
(
  cd "$(dirname "${SETUP}")"
  bash "${SETUP}" uninstall >/dev/null 2>&1 || true
)

if [[ -n "${STOCK_STEAM}" ]] \
  && { [[ ! -e "${SYSTEM_STEAM}" ]] \
       || grep -q 'slsteam-moon system launcher shim' "${SYSTEM_STEAM}" 2>/dev/null; }; then
  info "Restoring the stock /usr/bin/steam launcher"
  sudo install -o root -g root -m 755 "${STOCK_STEAM}" "${SYSTEM_STEAM}"
fi

info "Installing the x86_64 SLSsteam Moon runtime"
# --no Lumen / --no plugin: setup.sh installs only the runtime and wrapper.
(
  cd "$(dirname "${SETUP}")"
  bash "${SETUP}" install
)

is_elf64 "${SLS_SO}" \
  || die "SLSsteam.so is missing or not x86_64 after installation"
is_elf64 "${SLS_INJECT}" \
  || die "library-inject.so is missing or not x86_64 after installation"
[[ -x "${SLS_WRAPPER}" ]] \
  || die "SLSsteam wrapper was not created"

printf '\nSLSsteam Moon x86_64 installed at %s\n' "${SLS_DIR}"
printf 'Lumen and the LuaTools plugin were not installed by SteamTron.\n'
printf 'Steam stplug-in scripts were left untouched.\n'
printf 'Start Steam through the SLSsteam wrapper or from the desktop menu.\n'
