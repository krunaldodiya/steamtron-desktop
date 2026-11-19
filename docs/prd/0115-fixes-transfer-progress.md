# Progress for fixes import and export

- Status: done
- Date: 2026-11-19
- Areas: `crates/steamtron-core/src/fixes.rs`,
  `crates/steamtron-library/src/managed.rs`,
  `crates/steamtron-desktop/src/main.rs`, `crates/steamtron-desktop/ui/app.js`

## Problem

Importing or exporting a large fixes TAR (around 1 GiB) showed only a spinning
button label, so there was no way to tell whether the transfer was advancing.
Downloads, backups, restores, and verification already have byte-based progress
bars and cancellation.

## Requirements

- R1: TAR export reports written and hashed bytes; TAR import reports archive
  bytes read.
- R2: The managed and legacy Fixes panes show an inline progress bar with
  percent, message, and a cancel button.
- R3: Cancellation stops the operation, removes staging/temporary files, and
  leaves the existing fixes untouched.
- R4: Progress is emitted at most about every 120 ms, not once per chunk.

## Acceptance criteria

- [x] Import and export show a moving inline bar for a large TAR.
- [x] Cancel aborts and leaves no partial fixes or destination file.
- [x] Progress callbacks are monotonic and end at 100%.

## Verification

- `cargo test -p steamtron-core fixes::tests::tar_progress_is_monotonic_and_cancellable`
  passes; cancellation removes the temporary output and leaves the destination
  absent.
- `cargo check` passes for core, library, and desktop.
- Installed 0.1.36, real 1.4 GiB export: 148 events, UI bar moved
  `0% → 1.75% → 3.70%` with `Exporting fixes…`; finished at
  `100% · Export complete`.
- Installed 0.1.36, real 1.4 GiB import: UI bar moved
  `0% → 4.92% → 9.71%` with `Importing fixes…`; imported 10 files.
- Cancellation path is covered by the core test that aborts after the first
  callback and asserts the destination TAR was not created.

## Links

- Related: `0112-fixes-tar-format`, download/backup operation UI.
