# Separate appmanifest and Lua/manifest generation

- Status: done
- Date: 2026-11-16
- Areas: `crates/steamtron-desktop/src/main.rs`,
  `crates/steamtron-desktop/ui/app.js`

## Problem

One “Regenerate manifests” button downloaded the Lua/depot bundle, rewrote local
manifests, and attempted Steam registration in one action. Users could not tell
whether the Steam appmanifest was missing or the Lua/depot bundle was missing,
and the button mixed two independent repairs.

## Goal

Game Settings exposes two actions:

1. **Generate app manifest** — recreate/return the Steam
   `appmanifest_<appid>.acf`.
2. **Generate Lua/manifests** — restore the local Lua and depot manifest files
   from the persistent bundle store, using the API only when the bundle is
   absent.

## Requirements

- R1: Lua/manifest generation uses `download_bundle` (persistent store first,
  API fallback) and writes local Lua + depot manifests only.
- R2: Appmanifest generation writes
  `<SteamTronLibrary>/steamtronapps/appmanifest_<appid>.json` with the UmuTron
  schema: game_id, title, steam_app_id, content/fixes/compatdata directories,
  launcher_relative_path, proton, launch_options and launch_targets. No Steam is
  required.
- R3: Appmanifest generation overwrites the local metadata atomically.
- R4: No Steam registration is required for either action.
- R5: Files tab shows two separate buttons with separate result messages.

## Acceptance criteria

- [ ] Generate Lua/manifests writes Lua and manifests without touching Steam.
- [ ] Generate app manifest creates or returns the `.acf`.
- [ ] The persistent bundle store avoids an API call when a bundle exists.
- [ ] Missing Lua/registration produces an actionable error.

## Verification

- `cargo check` passes.
- The managed appmanifest path is taken from `game.installation.manifest`, which
  resolves to `/mnt/krunal/SteamTronLibrary/steamtronapps/appmanifest_<appid>.json`
  for the current drive layout.
- Persistent bundle store contains 21 bundles from the export/migration test.
- Both commands are registered in the Tauri handler.
- Release built and installed; the Files tab exposes Generate app manifest and
  Generate Lua/manifests separately.

## Decisions and risks

- The local appmanifest JSON is SteamTron metadata, not a Steam-client `.acf`.
  Generating a Steam `.acf` still requires Steam registration and remains a
  separate Steam repair path.
- Lua generation stays independent of Steam registration so users can repair
  local files before registering.

## Links

- Related PRDs: `docs/prd/0035-persistent-bundle-store.md`
