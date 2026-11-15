# Shared operation manager for backup, restore, and verification

- Status: active
- Date: 2026-11-15
- Areas: `crates/steamtron-desktop/ui/app.js`,
  `crates/steamtron-desktop/ui/style.css`,
  `crates/steamtron-desktop/src/main.rs`

## Problem

Download uses a large manager with stages, progress, statistics, and a lower
activity list. Backup game, restore game, and verify files only replace a button
label or write a single inline status line. Long operations look frozen and do
not share the download manager's window, hierarchy, or step feedback.

## Goal

Backup game, restore game, and verify files open the same 1100×760 operation
window as downloads/global settings. Each shows the current stage, an
indeterminate progress bar while work is running, elapsed time, status, and a
step list that advances from the backend.

## Non-goals

- Pausing or cancelling backup/restore/verify; only downloads support pause.
- Exact byte percentages for backup/restore.
- Persisting backup/restore operations across app exit.
- Changing library-wide backup/restore in Settings.

## Requirements

- R1: Download, backup, restore, and verify use the same large modal dimensions
  as global Settings (up to 1100×760).
- R2: Backup, restore, and verify open an operation manager rather than only a
  busy button.
- R3: Backend emits `operation-progress` with operation, game ID, stage, status,
  and message.
- R4: The manager shows operation, game, elapsed, and status stat cells in one
  row.
- R5: The lower step list marks prior stages done, the current stage running,
  and later stages waiting.
- R6: Success shows Complete with a full bar; failure keeps the window open with
  the error stage.
- R7: Closing a running operation window does not cancel the backend operation;
  it finishes in the background.
- R8: Download manager retains its depot list and icon controls inside the same
  large shell.

## Acceptance criteria

- [ ] Download, backup, restore, and verify open the same large window size.
- [ ] Backup progresses through collect/write/finish steps.
- [ ] Restore progresses through read/extract/register/finish steps.
- [ ] Verify progresses through check/repair/finish steps.
- [ ] A failure remains visible in the manager.
- [ ] Closing the manager does not cancel the backend task.

## Verification

- `cargo check -p steamtron-desktop` passed.
- Rust tests 11/11 passed.
- JSDOM operation harness passed: manager opens, four stat cells and three steps
  render, stage event marks the prior step done, completion sets the bar
  determinate and status Complete, close removes the window.
- Existing navigation suites still pass.

## Decisions and risks

- Backup/restore/verify are monolithic blocking commands, so the manager starts
  with an indeterminate bar. Stage events are emitted at command boundaries and
  inside the blocking worker where possible.
- Closing the operation window leaves the backend command running; there is no
  cancel protocol for these commands.
- The download manager keeps pause/resume/cancel and per-depot rows; the shared
  shell only standardizes size, stats, progress, and the lower list.

## Links

- Related PRDs: `docs/prd/0024-download-manager.md`
- Implementation: `crates/steamtron-desktop/ui/app.js`,
  `crates/steamtron-desktop/src/main.rs`
