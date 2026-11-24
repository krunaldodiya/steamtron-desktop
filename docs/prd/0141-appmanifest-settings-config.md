# Per-game appmanifest as the settings config

- Status: active
- Date: 2026-11-24
- Areas: `steamtron-core` library load/save, `steamtron-library` backup and managed settings, `steamtron-desktop` restore and backup

## Problem

Per-game settings were stored in `%LOCALAPPDATA%\SteamTron\library.json`, while
`<drive>/SteamTronLibrary/steamtronapps/appmanifest_<id>.json` was only a
metadata snapshot. Changing a launcher or another setting updated only
`library.json`; the appmanifest stayed stale until the user pressed
**Generate app manifest** or restored a backup.

This caused the Max Payne 3 restore failure:

- The live library target was `Max Payne 3\PlayMaxPayne3.exe` and the game ran
  with it.
- The backup taken at `G:\Games\SteamTronGames\Max Payne 3` recorded
  `Max Payne 3\MaxPayne3.exe` only.
- Restore replaced the live target with the backup target, so the game asked
  for the Rockstar launcher.
- Clicking **Generate app manifest** updated
  `D:\SteamTronLibrary\steamtronapps\appmanifest_204100.json`; changing the
  launcher alone did not.

The user-visible expectation is that the per-game appmanifest is the game's
config file, and every setting is read from it.

## Goal

The per-game `appmanifest_<id>.json` is authoritative for a managed game's
settings. Every settings change writes it, and every library load/play reads it.
`library.json` remains the index/cache, global settings, registration state and
fields that are not per-game settings.

## Non-goals

- No game-specific target patches or per-game catalog workarounds. The Max Payne
  catalog target added earlier is removed.
- No change to DLC/update metadata, which lives in the `*.cache.json` sidecar.
- No removal of `library.json`; it still indexes drives, games, Steam
  registration, artwork and metadata source.

## Requirements

- R1: The appmanifest contains every per-game setting: title, App ID, content
  and compatdata paths, launcher targets and arguments, Proton, prefix mode,
  environment, prelaunch, and fixes enabled.
- R2: `Library::open` overlays the appmanifest onto the library entry, so the
  config file wins over stale library fields.
- R3: `Library::save` writes the appmanifest from the live entry before writing
  `library.json`, so any settings mutation persists both.
- R4: Restore applies every setting from the backup's appmanifest, but keeps an
  existing launcher whose executable still exists. Backup targets are fallback;
  auto-detection is the final fallback.
- R5: Backup creation builds the manifest from the live entry, not from a stale
  file.
- R6: Older partial manifests and old backups still load. Missing fields keep
  the library value; an empty legacy `launch_targets` array does not erase
  configured launchers.
- R7: Metadata import keeps the imported appmanifest bytes exact and the next
  open overlays it.
- R8: If a manifest still contains both `launcher_targets` and the legacy
  `launch_targets`, prefer the list whose primary executable matches
  `launcher_relative_path`; saving removes the legacy key.

## Acceptance criteria

- [x] A round-trip test covers every manifest setting.
- [x] A load test proves manifest values override `library.json`.
- [x] A save test proves a settings change rewrites the appmanifest.
- [x] Launcher-change tests read the updated manifest without pressing
      **Generate app manifest**.
- [x] Restore target selection prefers an existing usable target, then backup
      targets, then auto-detection.
- [x] A dual-key manifest resolves to the list matching
      `launcher_relative_path`, and saving removes the legacy key.
- [ ] Runtime: change Proton, environment, arguments, prelaunch and launcher in
      the installed app and confirm `appmanifest_<id>.json` changes each time.
- [ ] Runtime: back up and restore Max Payne 3 and confirm the appmanifest and
      Play both keep `PlayMaxPayne3.exe`.

## Verification

- `cargo check --workspace` — exit 0 (0.1.63).
- `cargo test -p steamtron-core --lib -- --skip terminates_a_process_group` —
  59 passed, 0 failed.
- `cargo test -p steamtron-library --lib` — 41 passed, 0 failed.
- The process-group test failure is environmental (`kill -TERM` in the WSL test
  harness) and unrelated to this change.

0.1.63 was installed and its manifest was inspected on 2026-11-24. The live
Max Payne manifest contained both keys: `launcher_targets` still had the stale
`MaxPayne3` primary, while `launch_targets` had the correct `PlayMaxPayne3`.
0.1.64 resolves the conflict from `launcher_relative_path` and removes the
legacy key on save.

Runtime verification of 0.1.64 is pending until it is installed.

## Decisions and risks

- The manifest is authoritative; `library.json` is a cache/index. A settings
  mutation that bypasses `Library::save` would be overwritten on the next load.
  All desktop and managed-library settings paths now go through the shared save.
- Relative paths resolve against the manifest's parent on load, and against the
  local library root during restore. The Windows `\\?\` prefix is preserved.
- Linked games without a managed appmanifest keep their `library.json`
  settings.
- Manifest writes happen before `library.json`; a failed write leaves the old
  index in place instead of claiming a setting that was not persisted.

## Links

- `docs/prd/0136-launcher-bypass-registry.md`
- `docs/prd/0137-app-wide-real-path-manager.md`
- Commit `9342b67` (restore junction handling)
