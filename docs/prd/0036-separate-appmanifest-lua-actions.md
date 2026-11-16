# Remove the unused appmanifest action

- Status: done
- Date: 2026-11-16
- Areas: `crates/steamtron-desktop/src/main.rs`,
  `crates/steamtron-desktop/ui/app.js`

## Problem

The Files tab had a Generate app manifest action. The generated
`appmanifest_<appid>.json` is not read anywhere in SteamTron; only its **path** is
used to derive the Lua/layout base. The action added a second, unused repair
button and implied that Steam metadata generation was a required step.

## Goal

Remove the unused appmanifest action and keep only Generate Lua/manifests, which
restores the local Lua and depot files from the persistent bundle store. Existing
appmanifest JSON files are left on disk untouched.

## Requirements

- R1: Lua/manifest generation uses `download_bundle` (persistent store first,
  API fallback) and writes local Lua + depot manifests only.
- R2: Remove the Generate app manifest button, handler and Tauri command.
- R3: Keep `installation.manifest` as a path for layout/Lua discovery; do not
  delete existing `appmanifest_<appid>.json` files.
- R4: Files tab keeps one Generate Lua/manifests action.
- R5: Lua generation still uses the persistent bundle store before the API.

## Acceptance criteria

- [x] Generate Lua/manifests writes Lua and manifests without touching Steam.
- [x] The Generate app manifest button and command are removed.
- [x] Existing appmanifest JSON files remain untouched.
- [x] The persistent bundle store avoids an API call when a bundle exists.

## Verification

- `cargo check` passes.
- `rg generate_managed_appmanifest` returns no code references after removal.
- Files tab renders one Generate Lua/manifests button.
- Persistent bundle store contains 21 bundles from the migration/export test.

## Decisions and risks

- The managed appmanifest JSON is passive metadata. No code reads its contents;
  `installation.manifest` is used only for its parent path.
- A Steam-client `.acf` is a different file and remains handled by Steam
  registration/repair, not this action.
- Lua generation stays independent of Steam registration so users can repair
  local files before registering.

## Links

- Related PRDs: `docs/prd/0035-persistent-bundle-store.md`
