# Shared operation manager for backup, restore, and verification

- Status: done
- Date: 2026-11-15
- Areas: `crates/steamtron-desktop/ui/app.js`,
  `crates/steamtron-desktop/ui/style.css`,
  `crates/steamtron-desktop/src/main.rs`

## Problem

Download uses a large manager with stages and progress. Backup game, restore
game, and verify files only replace a button label or write a single inline
status line. Long operations look frozen and do not share the download window.
An early operation-manager draft showed an animated indeterminate bar, several
stat boxes, and a step list with multiple bars; that is more UI than the task
needs and does not show real progress.

## Goal

Backup game, restore game, and verify files open a large operation window with
one real 0–100% progress bar, a stage message, and nothing else. The same
single-bar progress appears inline under each action while the button uses a
present-participle label.

## Non-goals

- Pausing or cancelling backup/restore/verify; only downloads support pause.
- Exact byte percentages for backup/restore.
- Persisting backup/restore operations across app exit.
- Changing library-wide backup/restore in Settings.

## Requirements

- R1: Download, backup, restore, and verify use a large 1100px-wide modal.
  Download remains 1100×760; operations size to their single-bar content.
- R2: Backup, restore, and verify open an operation manager rather than only a
  busy button.
- R3: Backend emits `operation-progress` with operation, game ID, stage, status,
  message, and a real percent where available.
- R4: The operation window shows exactly one progress bar and one stage line; no
  stat boxes, elapsed timer, or step list.
- R5: Backup computes percent from archive bytes; restore from extracted bytes;
  verify from depot/file progress.
- R6: Success shows 100% and Complete; failure keeps the window open with the
  error stage.
- R7: Closing a running operation window hides it without cancelling; the
  action button stays enabled with its running label and reopens the same
  manager instead of starting a duplicate operation.
- R8: Download manager retains its own depot list and icon controls inside the
  large shell.
- R9: Buttons switch to running labels (`Backing up…`, `Verifying…`,
  `Restoring…`) and revert afterward.
- R10: An inline 0–100% bar appears under each action, matching download's
  inline bar.
- R11: The inline bar shows the backend stage and final message, then hides.
- R12: Backup, restore, and verify expose Pause/Resume and Cancel beside the
  progress bar, like downloads.
- R13: Backup/restore pause cooperatively between archive/extraction entries;
  verify pauses/resumes the SMD process group with signals.
- R14: Cancel stops the worker, removes a partial backup archive, and reports
  Cancelled without being overwritten by a failure.

## Acceptance criteria

- [ ] Download, backup, restore, and verify open the same large window size.
- [ ] Backup advances a real percent from archive bytes.
- [ ] Restore advances a real percent from extracted bytes.
- [ ] Verify advances a real percent from depot/file progress.
- [ ] The operation window has one bar and no stat/step boxes.
- [ ] A failure remains visible in the manager.
- [ ] Closing the manager does not cancel the backend task.
- [ ] Clicking the running action reopens the manager without starting another
  operation.
- [ ] Buttons change to running labels and back.
- [ ] Inline progress appears under backup, verify, and restore actions.

## Verification

- `cargo check -p steamtron-desktop` passed.
- Rust tests 11/11 passed.
- JSDOM operation harness passed: manager opens with a single percent bar and no
  stat boxes, a 42% event updates the bar/percent, completion shows 100%, and
  close removes the window.
- JSDOM inline harness passed: the verify button switches to "Verifying files
  integrity…", the inline bar receives 55% and the stage, returns the button to
  its idle label, and shows the completion message.
- Existing navigation suites still pass.
- Release built and installed; the download/operation windows use the same
  1100×760 shell as global Settings.

## Decisions and risks

- Backup enumerates files first and reports percent by written bytes. Restore
  counts archive bytes first, then reports extracted bytes. Verify streams SMD
  JSON and reports depot/file progress.
- Closing the operation window hides it; the backend command keeps running.
  The action button remains enabled and reopens the same manager. A second
  click never starts a duplicate operation.
- The download manager keeps pause/resume/cancel and per-depot rows; operation
  windows stay minimal like Steam's validation progress.
- Backup/restore pause at file/entry boundaries, not inside a single large file.
  Verify pauses the SMD process group, including DepotDownloaderMod.
- Cancelling restore leaves already-extracted partial files; cancelling backup
  removes its partial archive.

## Links

- Related PRDs: `docs/prd/0024-download-manager.md`
- Implementation: `crates/steamtron-desktop/ui/app.js`,
  `crates/steamtron-desktop/src/main.rs`
