# Separate appmanifest and Lua/manifest generation

- Status: active
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
- R2: Appmanifest generation uses the existing Steam managed-app entry and
  `repair_missing_appmanifests`; it does not download or rewrite Lua/manifests.
- R3: Appmanifest generation returns the existing path when the `.acf` already
  exists.
- R4: Appmanifest generation explains that Lua/registration must exist first when
  no Steam library entry is found.
- R5: Files tab shows two separate buttons with separate result messages.

## Acceptance criteria

- [ ] Generate Lua/manifests writes Lua and manifests without touching Steam.
- [ ] Generate app manifest creates or returns the `.acf`.
- [ ] The persistent bundle store avoids an API call when a bundle exists.
- [ ] Missing Lua/registration produces an actionable error.

## Verification

- `cargo check` passes.
- Persistent bundle store contains 21 bundles from the export/migration test.
- Both commands are registered in the Tauri handler.

## Decisions and risks

- Appmanifest repair remains Steam-backend dependent; it is not fabricated from
  a Lua file alone.
- Lua generation stays independent of Steam registration so users can repair
  local files before registering.

## Links

- Related PRDs: `docs/prd/0035-persistent-bundle-store.md`
