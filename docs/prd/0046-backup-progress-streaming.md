# Stream backup progress through large files

- Status: done
- Date: 2026-11-16
- Areas: `crates/steamtron-desktop/{src/main.rs,ui/app.js}`

## Problem

Little Nightmares contains a single 9.56 GB `.pak`. `archive_component` used
`tar.append_file`, which writes the entire file before updating `written`. The
UI stayed at 2% for the whole file, then jumped to 100%. Closing and reopening
the operation dialog also showed the stale 2%.

## Requirements

- R1: Count bytes while the file is being copied, not after it completes.
- R2: Emit backup progress at most every 200 ms during large files.
- R3: Honour cancellation while a large file is being copied.
- R4: Reattach a hidden operation dialog when its button reopens it.

## Verification

- `progress_reader_reports_incremental_chunks_and_honours_cancel` passes:
  1 MiB read produced more than one callback and cancellation aborts a read.
- `archive_component_writes_the_whole_file_and_finishes_at_100_percent`
  passes: the 1 MiB file is fully restored from the tar and the final emit is
  100%.
- Desktop tests pass.
- A full Little Nightmares backup was not run in this session; its 9.56 GB pak
  is the file that previously exposed the jump.

## Decisions and risks

- Progress now reflects bytes read into the tar, so it advances through a
  single large file.
- Tar headers use size and mode 0644; the previous `append_file` metadata path
  is replaced.
