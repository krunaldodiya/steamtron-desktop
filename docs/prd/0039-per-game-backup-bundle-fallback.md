# Per-game backup Lua/manifest fallback

- Status: done
- Date: 2026-11-16
- Areas: `crates/steamtron-desktop/src/main.rs`,
  `crates/steamtron-library/src/lib.rs`

## Problem

Per-game backups built `manifest-bundle.tar` from the game's local Lua file.
After the library Lua folder was removed, a game such as The Binding of Isaac
had no local Lua file, so its backup contained only content and fixes. Little
Nightmares had been backed up earlier and still contained Lua/materials.

## Requirements

- R1: Backups use the persistent global bundle when no local Lua file exists.
- R2: `manifest-bundle.tar` includes `<appid>.lua`, `<appid>.smd.lua`, and
  every depot manifest from the bundle.
- R3: Depot manifests prefer the per-game global folder and fall back to the
  bundle contents.
- R4: Temporary manifest files are removed before the backup folder is moved
  into place.

## Verification

- `backup_includes_lua_smd_lua_and_bundle_manifests` passes and asserts all
  three entry types.
- Isaac's persistent bundle contains `113200.lua` plus seven depot manifests;
  the helper now consumes that bundle when no local Lua exists.

## Decisions and risks

- The backup remains format 2; the manifest bundle keeps the existing
  `manifests/<title>/` layout, so existing restores remain compatible.
- SMD Lua is regenerated from the raw Lua rather than stored separately in the
  backup source.
