# Repair backup fixes from installed fixes

- Status: done
- Date: 2026-11-16
- Areas: `crates/steamtron-library/src/backup.rs`,
  `crates/steamtron-library/Cargo.toml`

## Problem

Three installed games had fixes missing or incomplete in their external backup:

- Watch Dogs 2: 93 installed files, no `fixes.tar`
- Rise of the Tomb Raider: 7 installed files, no `fixes.tar`
- Miles Morales: 5 installed files, 2 in `fixes.tar`

## Requirements

- R1: Build `fixes.tar` from the installed fixes directory.
- R2: Preserve files already present in the old `fixes.tar` and merge current
  installed files over them.
- R3: Update the appmanifest `components.fixes.sha256` and
  `fixes_directory`.
- R4: Never open or rewrite `content.tar`.

## Verification

- Actual repair: scanned 31, updated 4, already complete 12, skipped 15
  (not installed or no fixes).
- Watch Dogs 2: 93 files, hash matches appmanifest.
- Rise of the Tomb Raider: 7 files, hash matches.
- Miles Morales: 5 files, hash matches.
- FIFA 22 was also refreshed because its installed names were not fully present.
- Re-audit: every installed game with fixes now has a backup containing at least
  that many files.
- Newest `content.tar` remained `10:40`; repaired `fixes.tar` files were written
  at `10:57–10:58`.
- `fixes_tar_round_trips_files` passes.

## Decisions and risks

- Old fixes files not present in the current install are retained in the backup.
- Games that are not currently installed are skipped.
