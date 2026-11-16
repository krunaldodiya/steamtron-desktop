# Backend install requires Steam

- Status: done
- Date: 2026-11-16
- Areas: `crates/steamtron-desktop/{src/main.rs,ui/app.js}`

## Problem

The Doctor repair action could attempt to install SLSsteam Moon when Steam was
not installed. SLSsteam Moon installs into the Steam root, and SteamTron does
not install Steam itself.

## Requirements

- R1: `repair_steam_library` returns a clear error when `steam_root()` fails.
- R2: The error says Steam must be installed and started once, and that
  SteamTron does not install Steam.
- R3: Repair confirmations say the backend installs into an existing Steam
  installation, not that it installs Steam.

## Verification

- `cargo check -p steamtron-desktop` passes.
- `node --check crates/steamtron-desktop/ui/app.js` passes.
- On this host `steam_root()` already returns “Steam installation was not
  found”; the new guard is the first operation in the repair task.

## Decisions and risks

- The guard is not exercised through the Tauri command in this session because
  that requires the GUI/auth path; the Steam detection result and compiled
  guard were verified.
- Linux backend files live under `~/.local/share/SLSsteam` and
  `<steam_root>/config/stplug-in`. Windows BetterSteamTools installation is not
  implemented yet.
