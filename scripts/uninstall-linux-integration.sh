#!/usr/bin/env bash
# Complete uninstall of SLSsteam Moon, Lumen and the LuaTools integration.
#
# Keeps Steam, installed games, and by default Steam's config/stplug-in game
# scripts. Removes the system Steam launcher patch and restores the stock one.
#
# Usage:
#   ./uninstall-complete.sh
#
# Set SLSM_REMOVE_STPLUGIN=1 to also delete every game Lua script under
# <Steam>/config/stplug-in and <Steam>/config/lua. That is destructive.

set -euo pipefail

STEAM_ROOT=""
for candidate in \
  "${HOME}/.local/share/Steam" \
  "${HOME}/.steam/steam" \
  "${HOME}/.steam/root" \
  "${HOME}/.var/app/com.valvesoftware.Steam/.local/share/Steam"; do
  if [[ -d "${candidate}" ]]; then
    STEAM_ROOT="$(readlink -f "${candidate}")"
    break
  fi
done

die() {
  printf 'error: %s\n' "$*" >&2
  exit 1
}

info() {
  printf '\n==> %s\n' "$*"
}

warn() {
  printf 'warning: %s\n' "$*" >&2
}

require() {
  command -v "$1" >/dev/null 2>&1 || die "required command not found: $1"
}

[[ -n "${STEAM_ROOT}" ]] || die "Steam installation was not found"
[[ -d "${STEAM_ROOT}" ]] || die "Steam root does not exist: ${STEAM_ROOT}"

printf 'Steam root: %s\n' "${STEAM_ROOT}"
printf 'Removing SLSsteam/Lumen/LuaTools. Steam and games are left alone.\n'
if [[ -t 0 ]]; then
  printf 'Press Enter to continue, or Ctrl-C to cancel. '
  read -r _ || true
fi

require curl
require file
require pgrep
require systemctl
require sudo

info "Stopping Steam and Lumen"
pkill -TERM -x steam 2>/dev/null || true
pkill -TERM -x steamwebhelper 2>/dev/null || true
pkill -TERM -x lumen 2>/dev/null || true
sleep 2
pkill -KILL -x steam 2>/dev/null || true
pkill -KILL -x steamwebhelper 2>/dev/null || true
pkill -KILL -x lumen 2>/dev/null || true

systemctl --user disable --now slsteam-desktop-guardian.service 2>/dev/null || true

SYSTEM_STEAM=/usr/bin/steam
SYSTEM_BACKUP="${HOME}/.local/share/SLSsteam/system-launcher-backup/usr/bin/steam.orig"
STOCK_STEAM=/tmp/slsteam-stock-steam.orig
SYSTEM_SHIM=0
if [[ -f "${SYSTEM_STEAM}" ]] \
  && grep -q 'slsteam-moon system launcher shim' "${SYSTEM_STEAM}" 2>/dev/null; then
  SYSTEM_SHIM=1
  rm -f "${STOCK_STEAM}"
  if [[ -f "${SYSTEM_BACKUP}" ]]; then
    cp -a "${SYSTEM_BACKUP}" "${STOCK_STEAM}"
  else
    warn "the SLSsteam system shim is installed but its backup is missing"
  fi
fi

info "Running the x86_64 uninstaller"
if curl -fsSL "https://github.com/ogb4n/slsteam-moon-x64/releases/latest/download/uninstall-x86_64.sh" -o /tmp/slsteam-uninstall-x64.sh; then
  bash /tmp/slsteam-uninstall-x64.sh || warn "64-bit uninstaller reported a problem"
else
  warn "64-bit uninstaller could not be downloaded"
fi

info "Running the upstream uninstaller"
if curl -fsSL "https://raw.githubusercontent.com/swwayps/luatools-moon/main/uninstall.sh" -o /tmp/luatools-uninstall.sh; then
  bash /tmp/luatools-uninstall.sh || warn "upstream uninstaller reported a problem"
else
  warn "upstream uninstaller could not be downloaded"
fi

info "Removing known leftovers"
rm -rf \
  "${HOME}/.local/share/SLSsteam" \
  "${HOME}/.local/share/Lumen" \
  "${HOME}/.config/SLSsteam" \
  "${HOME}/.local/state/slsteam-moon"
rm -f "${HOME}/.SLSsteam.log" "${HOME}/.lumen.log"

for unit in slsteam-desktop-guardian.timer slsteam-desktop-guardian.path slsteam-desktop-guardian.service; do
  systemctl --user disable --now "${unit}" 2>/dev/null || true
  rm -f "${HOME}/.config/systemd/user/${unit}"
done
systemctl --user daemon-reload 2>/dev/null || true

USER_DESKTOP="${XDG_DATA_HOME:-${HOME}/.local/share}/applications/steam.desktop"
if [[ -f "${USER_DESKTOP}" ]] \
  && grep -q 'X-SLSteamMoon-Patched=true' "${USER_DESKTOP}" 2>/dev/null; then
  rm -f "${USER_DESKTOP}"
fi

if [[ "${SYSTEM_SHIM}" -eq 1 ]]; then
  if [[ ! -e "${SYSTEM_STEAM}" ]] \
    || grep -q 'slsteam-moon system launcher shim' "${SYSTEM_STEAM}" 2>/dev/null; then
    info "Restoring the stock /usr/bin/steam launcher"
    [[ -f "${STOCK_STEAM}" ]] || die "cannot restore /usr/bin/steam: its backup is missing"
    sudo install -o root -g root -m 755 "${STOCK_STEAM}" "${SYSTEM_STEAM}"
    rm -f "${STOCK_STEAM}"
  fi
fi

if [[ "${SLSM_REMOVE_STPLUGIN:-0}" == "1" ]]; then
  info "Removing game Lua scripts"
  rm -rf "${STEAM_ROOT}/config/stplug-in" "${STEAM_ROOT}/config/lua"
fi

printf '\nSLSsteam Moon, Lumen and the LuaTools integration were removed.\n'
printf 'Steam: %s\n' "${STEAM_ROOT}"
printf 'Game stplug-in scripts were %s.\n' \
  "$([[ "${SLSM_REMOVE_STPLUGIN:-0}" == "1" ]] && printf 'removed' || printf 'preserved')"
