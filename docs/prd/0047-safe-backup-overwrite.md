# Safe backup overwrite

- Status: done
- Date: 2026-11-16
- Areas: `crates/steamtron-desktop/src/main.rs`

## Problem

Overwrite deleted the existing backup before renaming the new one into place.
A crash or rename failure in that window could lose the previous backup.
Cancellation during copying was already safe because the new backup is built in
a staging directory.

## Requirements

- R1: Never delete the existing backup before the new tree is complete.
- R2: Move the old backup to a hidden sibling before renaming the new one.
- R3: Restore the old backup if renaming the new one fails.
- R4: Remove the old sibling after a successful replacement.

## Verification

- `replacing_a_backup_keeps_the_new_tree_and_removes_the_previous` passes:
  final contains the new file, the old file is gone, and no `.previous-*`
  tree remains.
- Backup archive and progress tests still pass.

## Decisions and risks

- Cancellation during copying leaves the old backup untouched and deletes only
  staging.
- Cancellation is not checked during the final rename. If the process is killed
  between moving the old backup and renaming the new one, the old backup remains
  at `.<name>.previous-<pid>`; it is not corrupted, but needs manual restoration.
