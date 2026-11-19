# Game-named fixes archive in backups

- Status: active
- Date: 2026-11-19
- Areas: `crates/steamtron-desktop/src/main.rs`,
  `crates/steamtron-library/src/backup.rs`

## Problem

New backups wrote the fixes component as `fixes.tar` while the content and
manifest components already use game-specific names. Restore and repair also
hard-coded `fixes.tar`, so the name was part of the backup contract even though
the manifest records it under `components.fixes.name`.

## Requirements

- R1: New backups write `<game name>.tar` and record that name in
  `components.fixes.name`.
- R2: Restore reads the component name from the manifest; old backups with
  `fixes.tar` keep working.
- R3: Backup repair writes the new name, updates the manifest, and removes the
  old `fixes.tar` after a successful write.
- R4: Component names are reduced to a file name, so a malformed manifest
  cannot escape the backup folder.

## Acceptance criteria

- [x] Manifest component resolution strips directories and falls back to
  `fixes.tar`.
- [x] Name sanitization keeps `Assassin's Creed Origins.tar` and turns
  `A/B: C?` into `A B C.tar`.
- [ ] Runtime backup writes the game-named TAR and restore accepts both names.

## Verification

- `cargo test -p steamtron-desktop fixes_archive_name` passes.
- `cargo test -p steamtron-desktop backup_component_name` passes.
- `cargo test -p steamtron-library backup::import_tests::fixes_tar_round_trips_files`
  passes with an explicit component name.

## Links

- Related: `0112-fixes-tar-format`.
