# Layer fixes over Windows games with a WinFsp union

- Status: done
- Date: 2026-11-18
- Areas: `crates/steamtron-overlay`, `crates/steamtron-desktop`, `crates/steamtron-core`

## Problem

Windows Play did not use the overlay: `play_managed` hard-disabled it on
`target_os = "windows"`, so managed games needed the fix DLL copied into the
game content, breaking the "keep Steam files pure" guarantee that Linux already
has through fuse-overlayfs. Two further problems appeared while bringing the
union up:

- Mounting WinFsp on a directory (`%LOCALAPPDATA%\steamtron\mount\…`) failed on
  some hosts with `STATUS_OBJECT_NAME_COLLISION` (`0xD0000035`).
- `OverlayGame::rewrite_arg` compared raw strings. Windows hands back verbatim
  `\\?\D:\…` paths from canonicalization while Play passes `D:\…`, so the
  rewrite silently no-oped and the game ran from the untouched content.

The fixes pack had its own defects, observed with Little Nightmares
(app 424840):

- Removing GoldBerg reported "no GoldBerg applied" even though the UI had the
  button enabled, because the pack scan stopped six levels deep and missed
  `Engine/Binaries/ThirdParty/Steamworks/Steamv132/Win64/steam_settings/…`.
- Applying GoldBerg rewrote `steam_interfaces.txt` with the emulator DLL's
  exported symbol names. GSE expects interface versions (`SteamUtils007`) and
  exited after the splash with "Missing interface".
- Steamless only accepted `target.executable.is_file()`, so a library imported
  from another machine (`/mnt/…` or `D:/mnt/…`) failed with "No launcher was
  found inside this game's folder".

## Goal

Windows Play launches the game from a merged view of the untouched install and
the fixes pack, exactly as Linux does, and the GoldBerg/Steamless actions work
on packs and launch targets that carry imported paths.

## Non-goals

- Removing the per-game fix copy from existing libraries.
- Changing the Linux fuse-overlayfs path.
- Bumping the release version or publishing a new installer.

## Requirements

- R1: `play_managed` uses the overlay on Windows when the fixes pack is non-empty.
- R2: The Windows overlay mounts on a free drive from `S:` to `Z:` and passes no
  Proton arguments on Windows.
- R3: Argument and working-directory rewriting handles verbatim prefixes and
  Windows case-insensitivity.
- R4: The fixes scan reaches UE4 emulator layouts and recognizes a bare
  `steam_api64.dll` swap as a payload directory.
- R5: Applying GoldBerg preserves the pack's `steam_interfaces.txt` and never
  writes exported symbol names.
- R6: Steamless and GoldBerg resolve registered launch targets against the
  current library root.

## Acceptance criteria

- [x] Windows Play merges fixes over the untouched content and the game starts.
- [x] The union mounts on a drive letter and unmounts cleanly.
- [x] Remove GoldBerg deletes the applied DLL and `steam_settings` instead of
  reporting "no GoldBerg applied".
- [x] Applying GoldBerg leaves GSE's interface list intact and the game reaches
  gameplay instead of exiting after the splash.
- [x] A library restored with a foreign absolute launcher path can run Steamless.

## Verification

- User confirmed in the prior session that the game launched through the
  Windows overlay ("now game working fine", 2026-11-18 10:55Z) and that Remove
  GoldBerg removed the applied fix.
- Fixes pack after the removal/re-apply cycle contains the expected
  `Engine/Binaries/ThirdParty/Steamworks/Steamv132/Win64/steam_api64.dll` and
  `steam_settings/steam_appid.txt`, with a manifest listing exactly those files.
- `cargo test -p steamtron-overlay --lib`: 2 passed
  (`rewrite_arg` verbatim/case tests).
- `cargo test -p steamtron-desktop --bin steamtron-desktop goldberg_tests`:
  2 passed (deep UE4 layout, bare DLL swap).
- `cargo test -p steamtron-core rebases_an_absolute_target_to_the_managed_root`:
  passed with a Windows `D:/mnt/…` source path.
- `steamtron-core` still fails four unrelated Linux-only tests on Windows
  (`plans_proton_with_prefix_and_app_id`, `resolves_a_proton_runner_directory`,
  `reports_missing_executable_and_content`, `default_drive_is_unique`). They
  fail on HEAD too; path handling is Unix-only and is not touched by this work.
- Installed binary `C:\Program Files (x86)\SteamTron\steamtron-desktop.exe`
  rebuilt after the changes (20,579,840 bytes).

## Decisions and risks

- Drive-letter mount (`S:`–`Z:`) instead of a directory mount point; WinFsp
  exercises that layout and it avoids the reparse-point collision.
- Windows runs the merged executable directly; only Linux hands it to Proton.
- `steam_interfaces.txt` is never synthesized; a missing list falls back to
  GSE's built-in table.
- Risk: a host with all of `S:`–`Z:` occupied fails Play with a clear mount
  error instead of falling back to a directory mount.
- Risk: the fixes scan now walks 16 levels. Pathological packs cost more I/O
  during detection.

## Links

- Changelog: `[Unreleased]` in `CHANGELOG.md`.
- Related: `0077-windows-native-launch`, `0083-windows-ddm-dotnet`,
  `0093-steam-dlc-parity`.
