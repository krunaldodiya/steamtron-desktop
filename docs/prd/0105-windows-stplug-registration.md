# Windows registration writes stplug-in

- Status: done
- Date: 2026-11-18
- Areas: `crates/steamtron-register/src/windows.rs`

## Problem

After installing BetterSteamTools and syncing, Steam's library showed
**All Games (0)** although `Steam/config/lua/*.lua` contained the expected
`addappid` manifests. Copying the same files to `Steam/config/stplug-in` and
restarting Steam made all four games appear.

The Windows adapter chose `config/stplug-in` only when that directory already
existed; otherwise it wrote `config/lua`. BetterSteamTools does not apply the
`config/lua` fallback on this Steam build, so registration reported success
while nothing reached Steam.

## Goal

A Windows sync always writes Lua manifests where BetterSteamTools watches them,
creating the directory when missing.

## Requirements

- R1: `BtsBackend::lua_dir()` is always `<steam>/config/stplug-in`.
- R2: `register` creates the directory through the existing atomic writer.
- R3: `verify`, `unregister`, and the reported `lua_dir` use the same path.

## Acceptance criteria

- [x] A unit test registers into `config/stplug-in/42.lua`.
- [x] A sync from the rebuilt app updates the stplug-in files.

## Verification

- Unit test `windows::tests::registers_verifies_and_removes_one_app` passes and
  asserts the stplug-in path.
- Manual evidence: copying the Lua files into `config/stplug-in` and restarting
  Steam changed the library from `All Games (0)` to `All Games (4)`
  (Cricket 26, Little Nightmares Enhanced/II/III), with no Millennium or
  LuaTools plugin installed.
- Rebuilt 0.1.28 binary, CDP `invoke('sync_steam_library')` after deleting
  `stplug-in/3468650.lua`: result
  `"Synced 1 game … 3 already registered; recreated 4 appmanifests."`; the file
  was recreated at 21:16 with identical content, and `config/lua` mtimes stayed
  20:16.

## Decisions and risks

- `config/lua` is no longer written on Windows; files already there are harmless
  but unused. The LuaTools plugin may still read it, so no migration deletes it.
- Steam must be restarted (or BetterSteamTools hot reload) to see new manifests.

## Links

- Related: `0104-bettersteamtools-1-0-9`, `0091-library-sync`.
