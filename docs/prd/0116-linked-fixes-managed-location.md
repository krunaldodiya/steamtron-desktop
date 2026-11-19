# Managed fixes folder for linked games

- Status: done
- Date: 2026-11-19
- Areas: `crates/steamtron-core/src/library.rs`,
  `crates/steamtron-library/src/managed.rs`,
  `crates/steamtron-desktop/src/main.rs`,
  `crates/steamtron-desktop/src/fix_tools.rs`

## Problem

A managed game imports fixes into
`D:\SteamTronLibrary\steamtronapps\fixes\<game>`, but Red Dead Redemption 2
(linked Steam install, root `\\?\D:\SteamLibrary\steamapps\common\Red Dead
Redemption 2`) imported them into `%LOCALAPPDATA%\steamtron\fixes\1174180`.
Its prefix and manifest already live under
`D:\SteamTronLibrary\steamtronapps`, so the fixes folder was split from the
rest of the game's SteamTron data.

`fixes_dir` only looked for a `SteamTronLibrary`/`UmuLibrary` ancestor of
`installation.root`. A linked game's root is Steam's library, so it always took
the app-data fallback added in 0109, even when the game is catalogued in a
managed library.

## Goal

Every game catalogued in a managed library stores fixes in
`<drive>\SteamTronLibrary\steamtronapps\fixes\<game title>`, regardless of where
its content is installed. Fixes already imported into the app-data fallback
move there automatically.

## Non-goals

- Changing the fallback folder for games with no managed prefix or manifest.
- Moving or rewriting fixes when the game title changes.
- Changing the TAR layout (0112) or the backup format (0114).

## Requirements

- R1: Resolve the managed drive from `installation.root`, then
  `installation.prefix`, then `installation.manifest`.
- R2: Use `game_layout(..).fixes` when any of those paths is inside a
  `SteamTronLibrary`/`UmuLibrary` tree; only fall back to
  `<data home>/fixes/<appid>` when none is.
- R3: On resolve, adopt an existing legacy app-data folder into the managed
  folder: rename when possible, otherwise copy into a staging folder beside the
  target and rename, skipping symbolic links. Never overwrite an existing
  managed folder.
- R4: Route fixes view, import, remove, export, Goldberg/Steamless helpers, and
  the Play overlay through the same helper.

## Acceptance criteria

- [x] Importing the RDR2 TAR puts `1911.dll`, `bink2w64.dll`,
  `installscript_sdk.vdf`, `Launcher.exe`, and `RDR2.exe` in
  `D:\SteamTronLibrary\steamtronapps\fixes\Red Dead Redemption 2`.
- [x] Existing `%LOCALAPPDATA%\steamtron\fixes\1174180` content moves to that
  folder the next time fixes are opened, imported, exported, or the game is
  played.
- [x] Managed games keep their current
  `SteamTronLibrary\steamtronapps\fixes\<title>` folder.
- [x] A linked game with no managed prefix/manifest still uses
  `<data home>\fixes\<appid>` and the Fixes tab keeps working.

## Verification

- `cargo test -p steamtron-core library::tests`: 13 passed; the three new tests
  `linked_game_with_managed_footprint_uses_the_library_fixes_folder`,
  `linked_game_without_a_managed_footprint_uses_app_data`, and
  `legacy_fixes_are_adopted_into_the_managed_folder` pass.
  `default_drive_is_unique` fails because `/mnt/one` is not an absolute path on
  Windows; it is unrelated to this change and fails on the previous commit too.
- Real library probe (`Library::load()` on
  `%LOCALAPPDATA%\steamtron\library.json`):
  `Assassin's Creed Origins => D:\SteamTronLibrary\steamtronapps\fixes\Assassin's
  Creed Origins` and `Red Dead Redemption 2 => D:\SteamTronLibrary\steamtronapps\
  fixes\Red Dead Redemption 2`.
- Real import through `steamtron_library::managed::install_fixes_with_progress`
  (the function behind the Fixes Import command) with
  `Downloads\Red Dead Redemption 2.tar`: returned
  `fixes path: D:\SteamTronLibrary\steamtronapps\fixes\Red Dead Redemption 2`
  with the five files; `%LOCALAPPDATA%\steamtron\fixes\1174180` no longer
  exists; no `.adopt-*` staging folder remains.
- SHA-256 of all five imported files matches the values verified from the ZIP
  (`8377CEC5…1911.dll`, `F6F8E7FE…bink2w64.dll`, `E3B0C442…installscript_sdk.vdf`,
  `6F20187D…Launcher.exe`, `9B8B8454…RDR2.exe`).
- Release binary built with `cargo build --release -p steamtron-desktop`
  (SHA-256 `4BBCF41F5C4ECAAFC7344D43BA48CBF9FD63DBB6243B66709928488A635FA1CD`)
  and installed to `%LOCALAPPDATA%\Programs\SteamTron\steamtron-desktop.exe`
  with the same hash. The import above used the library code, not a UI click;
  the Fixes tab was not driven on the installed build.
- Published `v0.1.37` from `fb6d0ba` (workflow run `44510274555`): Linux
  `.deb`, `.rpm`, `.AppImage`, and the Windows NSIS bootstrapper; downloaded
  SHA-256 of all four assets matches `SHA256SUMS.txt`.

## Decisions and risks

- The managed prefix and manifest are the existing markers SteamTron writes
  when it catalogues a Steam install, so they identify the owning library
  without a new stored field.
- Adoption is best effort: a failed copy leaves the legacy folder untouched and
  the managed folder absent, so nothing is deleted without a complete copy.
- Replaces the 0109 decision (R2) that all linked-game fixes live in app data.
- Risk: two catalogued games with the same title share one managed folder; the
  existing layout already had this constraint.

## Links

- Revises `0109-windows-parity-link-tray-fixes.md` R2.
- Related: `0111-large-fixes-import.md`, `0112-fixes-tar-format.md`,
  `0115-fixes-transfer-progress.md`.
