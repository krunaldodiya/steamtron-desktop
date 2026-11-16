# Current-file activity for backup and restore

- Status: done
- Date: 2026-11-16
- Areas: `crates/steamtron-desktop/{src/main.rs,ui/app.js,ui/style.css}`

## Problem

The operation dialog showed only the progress bar and generic stages such as
“Backing up game content…”. Users could not see which file was being archived
or restored, especially during a single multi-gigabyte file.

## Requirements

- R1: Backup activity names the file currently being archived.
- R2: Restore activity names the file currently being extracted.
- R3: Manifest and finalization stages get their own activity text.
- R4: Activity text sits below the progress bar and above the stats card.
- R5: Add breathing space around the activity line.

## Verification

- `archive_component_writes_the_whole_file_and_finishes_at_100_percent` now
  also asserts that an emitted activity contains `Archiving large.bin`.
- The activity line was moved below the progress row in the operation modal.
- CSS uses `margin:14px 0 18px` and `min-height:20px` for the activity line.
- Desktop tests: 17 passed; UI syntax check passes.
- The rendered spacing was not visually screenshotted in this session.
