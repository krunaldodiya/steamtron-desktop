# One consolidated library backup

- Status: done
- Date: 2026-11-15
- Areas: `crates/steamtron-library/src/backup.rs`, `crates/steamtron-desktop/ui/app.js`

## Problem

Settings exposed two overlapping backups: General → Maintenance backed up Lua
manifests, appmanifests, and fixes, while Storage → Library backup separately
exported `library.json`. Users had to keep two files to restore an install.

## Goal

One backup file contains the library index (drives, games, settings) plus Lua
manifests and appmanifests. It deliberately excludes game content and fixes,
which are covered by the per-game backup. Restoring it restores the library
first and then the metadata. Storage no longer duplicates the feature.

## Non-goals

- Backing up game content, prefixes, saves, or per-game fixes.
- Replacing the per-game backup ZIP in Storage.

## Requirements

- R1: Backup archive includes `library.json` and the metadata entries.
- R2: Export works when Steam is absent; Steam-only entries are skipped.
- R3: Export includes managed local Lua even without Steam.
- R4: Restore imports `library.json` first, then resolves Lua destinations from
  the restored library.
- R5: Missing Steam only skips appmanifest/Steam entries with a warning.
- R6: Fixes directories are never exported or restored by this backup.
- R7: Storage shows only drives, per-game backup, and data folder; General has
  the single Library backup row.

## Acceptance criteria

- [x] General shows one Library backup control.
- [x] Storage no longer shows Library file export/import.
- [x] Backup ZIP contains `library.json` when a library exists.
- [x] Restore imports the library and reports `library: true`.
- [x] Steam-less restore skips unavailable Steam files with warnings.
- [x] Fixes entries are excluded; legacy entries are skipped with a warning.
- [x] Existing dialog/settings behavior remains intact.

## Verification

- `cargo check -p steamtron-desktop` passed.
- Dialog UI suite 73/73; the backup section checks pass:
  `General shows one consolidated Library backup`,
  `Storage no longer duplicates library backup`,
  `Restore uses the compact confirm dialog`.
- Backup export/restore code compiled with optional Steam/Config/library
  targets; Rust library tests 23/23.

## Decisions and risks

- `library.json` is validated through the existing import path before metadata
  is written.
- The restore summary reports skipped entries; the UI shows the count.
- Local managed Lua is preferred when the restored library contains the game;
  Steam paths are fallbacks.

## Links

- Supersedes the Storage-only part of `docs/prd/0012-library-backup.md`.
- Related: `docs/prd/0018-steamless-operations.md`
