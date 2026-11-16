# Populate managed appmanifest files

- Status: done
- Date: 2026-11-16
- Areas: `crates/steamtron-library/src/{managed,backup}.rs`,
  `crates/steamtron-desktop/src/main.rs`

## Problem

Only one of 21 managed games had `appmanifest_<appid>.json` in
`SteamTronLibrary/steamtronapps`. The library entries pointed at the path, but
the file was never written unless a backup was restored.

## Requirements

- R1: Every managed game with an app ID gets an appmanifest JSON at
  `game.installation.manifest`.
- R2: Adding a game writes the appmanifest immediately.
- R3: Startup populates missing files for existing games.
- R4: Existing appmanifest files are never overwritten.
- R5: The JSON contains content, fixes, compatdata, launcher, launch targets,
  Proton, and launch options.

## Verification

- `populate_missing_appmanifests` wrote 20 files on this machine.
- All 21 library entries now have a present manifest file.
- `appmanifest_113200.json` contains Isaac's content/fixes/compatdata paths and
  `Isaac.exe` launch target.

## Decisions and risks

- The file is SteamTron's managed descriptor, not a Steam ACF; it does not
  imply that Steam is installed.
- Population is idempotent and runs in the startup background thread.
