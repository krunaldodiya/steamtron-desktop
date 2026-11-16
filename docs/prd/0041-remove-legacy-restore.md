# Remove the legacy consolidated restore

- Status: done
- Date: 2026-11-16
- Areas: `crates/steamtron-desktop/ui/app.js`,
  `crates/steamtron-desktop/src/main.rs`, `crates/steamtron-library/src/backup.rs`

## Problem

Settings still exposed “Restore legacy backup…”, which restored the old
consolidated ZIP (library index, appmanifests, Lua) and had nothing to do with
the per-game ZIPs produced by “Export games…”. The legacy path was confusing
and could overwrite library state.

## Requirements

- R1: Remove the legacy restore button and confirmation flow.
- R2: Remove the `restore_backup` and obsolete `export_backup` Tauri commands.
- R3: Delete the consolidated backup/restore implementation and its types.
- R4: Keep “Export games…” and the per-game backup/restore controls.

## Verification

- `grep` finds no `Restore legacy`, `restore_backup`, or `setting-restore`
  references.
- The only restore control left is the per-game “Restore backup folder…” in
  game settings.
- `node --check crates/steamtron-desktop/ui/app.js` passes.
- Desktop tests pass.

## Decisions and risks

- Global “Export games…” remains a metadata/Lua/manifest export, not game
  content. Full game content is backed up and restored per game from game
  settings.
- Existing consolidated backup ZIPs are no longer accepted by the UI.
