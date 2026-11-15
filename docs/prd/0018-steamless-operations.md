# Steam-independent managed games

- Status: active
- Date: 2026-11-15
- Areas: `crates/steamtron-library`, `crates/steamtron-core`, `crates/steamtron-desktop`

## Problem

Managed-game operations treated a Steam installation or registration backend
as mandatory. Adding a game failed when the backend was not ready; playing
always resolved a Steam root even when the game did not require Steam;
download, verify, manifest regeneration, updates, and DLC detection read the
Lua manifest only from Steam's `stplug-in`; Valve Proton listing failed without
a Steam library.

## Goal

Steam is optional. A game is added to the SteamTron library even when Steam is
absent. Steam registration is best-effort when a ready backend exists. Play,
download, verify, and manifest regeneration work from a local Lua manifest.
Only games whose settings say `requires_steam` check for an installed/running
Steam client.

## Non-goals

- Implementing per-DLC installation toggles. DLC detection now reads the local
  manifest, but the install checkbox remains informational.
- Downloading a missing selected Proton before Play. Play falls back to an
  installed runner; if none exists it reports a clear install-required error.

## Requirements

- R1: `add_managed_game` accepts no backend; it writes the library entry and a
  local Lua manifest under the managed game folder, and stores provider
  manifests in SteamTron's local manifest directory.
- R2: A ready Steam backend still registers the App ID and stores the pending
  restart state; registration failure is a warning, not an add failure.
- R3: Play resolves a Steam root only when `launch.requires_steam` is true.
  Non-Steam games launch with the configured Proton against their managed
  prefix; a missing configured runner falls back to any installed runner.
- R4: Download and file verification resolve the local Lua first, then Steam.
- R5: Manifest regeneration writes local Lua/manifests and optionally
  registers with Steam.
- R6: Updates and DLC detection read the local Lua first and scan the local
  manifest store.
- R7: Valve Proton releases are listed without a Steam library.
- R8: New games use the global default prefix source from Settings.

## Acceptance criteria

- [x] Adding with no backend creates a SteamTron library entry, local Lua, and
  no Steam registration.
- [x] Adding with a ready backend still registers the App ID.
- [x] Playing a non-Steam game does not require a Steam install.
- [x] A game with `requires_steam` still requires a running Steam client.
- [x] Verify files uses a local Lua without Steam.
- [x] Regenerate manifests writes local files and reports a Steam-less warning.
- [x] Updates/DLC read the local Lua; Valve Proton lists without Steam.
- [x] Global default prefix source is editable in General Settings.
- [ ] DLC install checkboxes are actionable.
- [ ] A missing selected Proton is downloaded automatically before Play.

## Verification

- `cargo check -p steamtron-desktop` passed.
- `cargo test -p steamtron-library --lib`: 23/23, including
  `adds_without_a_steam_backend_and_writes_local_lua`.
- `cargo test -p steamtron-core library::tests --lib`: 11/11.
- Dialog UI suite: 73/73; base 34/34; auth 13/13.

## Decisions and risks

- Registration failures are downgraded to warnings so the library entry is
  never lost because Steam is missing.
- The local Lua path is `SteamTronLibrary/steamtronapps/lua/<appid>.lua`; depot
  manifests live under the SteamTron data home.
- A fallback installed Proton is selected when the configured path is missing.
  Installing the exact missing runner from Play remains open.

## Links

- Related: `docs/prd/0006-store-cache.md`, `docs/prd/0019-consolidated-backup.md`
