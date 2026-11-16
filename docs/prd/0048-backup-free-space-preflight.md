# Backup free-space preflight

- Status: done
- Date: 2026-11-16
- Areas: `crates/steamtron-desktop/src/main.rs`

## Problem

A backup is built in a staging directory beside the existing backup. With a
50 GB backup already present and only 10 GB free, overwrite needs another full
50 GB for staging. The previous code discovered this only after spending time
archiving, or after a late write failure.

## Requirements

- R1: Estimate the staging size from content, fixes, manifests, per-file tar
  overhead, and a 32 MiB margin.
- R2: Read available space of the destination filesystem before archiving.
- R3: Abort with required/available sizes when space is insufficient.
- R4: Leave the existing backup untouched on this failure.
- R5: Skip the preflight when the platform cannot report free space.

## Verification

- `backup_size_and_free_space_are_reported` passes:
  - 1 GiB formats as `1.0 GiB`.
  - `statvfs` reports positive space for a temp directory.
  - `u64::MAX` required space returns an error containing
    “Not enough free space”.
- Desktop tests pass.

## Decisions and risks

- Overwrite still requires space for a complete second copy; the old backup is
  not deleted early to avoid data loss.
- Linux uses `statvfs`; non-Unix platforms currently skip the preflight.
