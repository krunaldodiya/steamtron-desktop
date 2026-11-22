# Automatic Windows Security exclusions for fixes

- Status: active
- Date: 2026-11-22
- Areas: `crates/steamtron-desktop/src/security.rs`,
  `crates/steamtron-desktop/src/main.rs`

## Problem

The scoped Defender exclusion feature did not protect managed fixes, and it
never ran for the operations that install them.

Evidence from 2026-11-22, Grand Theft Auto V Enhanced (3240220), a managed game
whose fixes were restored from a backup:

- `security::planned_paths` resolved the fixes folder from `Config` or
  `%LOCALAPPDATA%\steamtron\fixes\<appid>`; the managed folder
  `D:\SteamTronLibrary\steamtronapps\fixes\<game>` was never planned. This game
  has no legacy config entry, so the planned path did not exist.
- Only `install_fixes`/`use_fixes_folder` called `require_exception`; the
  managed import (`install_managed_fixes`) and `restore_game_backup` did not.
- Defender quarantined `PlayGTAV.exe` from the fixes folder at 13:40:33 right
  after the restore. Restoring the file made Play fail with
  `os error 225` (`ERROR_VIRUS_INFECTED`) from the overlay wrapper.
- Adding folder exclusions for the fixes directory and `S:\` did **not** stop
  it. Defender reports the overlay file as
  `\Device\Volume{…}\PLAYGTAV.EXE` (the WinFsp volume), so drive-letter and
  folder exclusions do not match. Adding **process** exclusions for
  `steamtron-desktop.exe`, `PlayGTAV.exe` and `GTA5_Enhanced.exe` made the game
  launch and run (verified by four `GTA5_ENHANCED`/`PLAYGTAV` processes from
  `S:`).
- Defender also hides the exclusion list from a non-elevated process
  (`Get-MpPreference` returns `N/A: Must be an administrator to view
  exclusions`), so the old `apply` verification always reported failure after a
  successful elevated change.

## Goal

Applying, importing or restoring fixes adds the required Defender exclusions
automatically (one elevation prompt per game), and the next Play of a fixed
game is not blocked by antivirus.

## Non-goals

- Disabling Defender, cloud protection, or tamper protection.
- Broad drive or Steam-root exclusions.
- Supporting third-party antivirus.
- Excluding files outside SteamTron's fixes, overlay and game folders.

## Requirements

- R1: Resolve the fixes folder from the library (`fixes_dir`) for managed and
  linked games, keep the legacy app-data folder when it exists, and include the
  game's installation root when the game folder is requested.
- R2: Exclude the overlay wrapper process (the running binary) and the game's
  executables: launch targets, `.exe` files under the game root and the fixes
  folder (depth ≤ 3, deduped case-insensitively, at most 48 names).
- R3: Run the exclusions automatically before a managed fixes import, a legacy
  fixes import/folder change, a backup restore that contains fixes, and an
  overlay launch of a game with fixes.
- R4: One elevation per game: store the applied paths/processes in
  `%LOCALAPPDATA%\steamtron\security\<appid>.json` and skip a game whose planned
  paths are already covered.
- R5: Treat a successful elevated script as success for a non-elevated app;
  the marker feeds the status view so it does not report phantom missing paths.
- R6: Removing the exception deletes the marker; repairing re-applies it.
- R7: Linux behavior is unchanged (the helpers compile but are not called).

## Acceptance criteria

- [ ] A restored/imported game with fixes gets folder and process exclusions
      without opening the Security panel.
- [ ] Grand Theft Auto V Enhanced launches with fixes after the automatic
      exclusions, with no manual `Add-MpPreference`.
- [ ] Declining the prompt fails with a clear message (no silent `os error
      225`).
- [ ] A game without fixes is unaffected.
- [ ] The Security status panel reflects the recorded exclusions.

## Verification

- Manual root cause run on 2026-11-22: path/drive exclusions alone still gave
  `os error 225`; adding process exclusions for the wrapper and the two GTA
  executables launched the game (processes and overlay mount observed).
- Detection history: `Trojan:Win32/Ravartar!rfn`, resource
  `\Device\Volume{…}\PLAYGTAV.EXE`, process
  `C:\Program Files (x86)\SteamTron\steamtron-desktop.exe`.
- Code: `cargo check -p steamtron-desktop` and the new security unit tests
  (to be recorded with the release).

## Decisions and risks

- Process exclusions are name-based and global; the list is bounded to the
  game's own executables so unrelated processes are not covered in practice.
  This is the only mechanism that matches SteamTron's WinFsp volume path.
- The marker is the app's record because Defender hides exclusions from a
  non-elevated process. Removing exclusions outside SteamTron leaves a stale
  marker; the Repair action re-applies them.
- The first Play/import/restore of a fixed game shows one UAC prompt. Declining
  stops that operation with an explanation instead of a later AV crash.
- Restore staging (`.restore-<pid>`) is still scanned during extraction; its
  files are moved into the excluded fixes folder at commit. If Defender
  quarantines a staged file, re-importing fixes with the exclusion active
  repairs it.

## Links

- `0106-antivirus-exclusions.md` (original scoped exclusion feature).
- `0116-linked-fixes-managed-location.md` (managed fixes folder).
- `0121-portable-backup-name-safety.md` (restore flow).
- Ghost of Tsushima/PSPC (`0122`) unrelated; GTA V Enhanced appid 3240220.
