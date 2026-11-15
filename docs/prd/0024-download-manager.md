# Reopenable download manager with pause, resume, and cancel

- Status: done
- Date: 2026-11-15
- Areas: `crates/steamtron-desktop/src/download.rs`,
  `crates/steamtron-desktop/src/main.rs`,
  `crates/steamtron-desktop/ui/app.js`

## Problem

Starting a managed download replaces the Download button with a disabled
"Downloading" button. Once the drive picker closes there is no control surface:
the inline progress bar cannot be opened, and the user cannot pause, resume, or
cancel. If the downloader exits without emitting a terminal event, the stale
in-memory state keeps the button permanently disabled, so the game appears stuck
at "Preparing depot downloads…" and the download cannot be retried.

A start failure such as a missing Lua manifest also left the in-memory
"starting" entry behind, so the detail page could stay on Downloading instead of
returning to Download.

## Goal

The Downloading button opens a download manager that shows progress and offers
Pause/Resume and Cancel. Stale or failed downloads return to a retryable Download
button, and a cancelled download removes its partial files.

## Non-goals

- Resuming a download after the SteamTron process exits.
- Changing SMD's download or verification behavior.
- Reimplementing the detached download worker.

## Requirements

- R1: The Downloading button is enabled and opens a download manager.
- R2: The manager uses a large broad download-modal layout (up to 1100px) with
  a progress bar, current stage message, depot/install progress, and five stat
  cells in one row for progress, downloaded bytes, total size, average speed,
  and ETA. It updates without closing.
- R2a: Pause/Resume and Cancel are icon-only buttons immediately after the
  progress bar. The manager has no bottom Close button; the top-right window
  close is the only close control.
- R2b: Depot progress is shown when SMD reports a depot count, matching
  UmuTron's data/install bar split.
- R2c: A live download with no output for 20 seconds reports its stalled time
  in the manager instead of appearing frozen.
- R3: Pause sends `SIGSTOP` to the downloader process group; Resume sends
  `SIGCONT`. The state reports `paused` and keeps partial files.
- R4: Cancel terminates the downloader, removes partial game files, and returns
  the detail page to Download.
- R5: `managed_download_status` reports `error` when a non-terminal state has no
  live downloader, including a recorded PID that no longer exists. The UI can
  then clear a stale Downloading state.
- R6: A failed `start_managed_download` clears the in-memory starting state and
  re-renders the Download button.
- R7: Closing the manager does not cancel the download; progress stays available
  from the Downloading button.
- R8: Removing a game from the library cancels its active download.
- R9: If a managed game's local Lua manifest is missing, start regenerates it
  from the signed-in manifest API before falling back to the re-add error.

## Acceptance criteria

- [ ] Downloading opens a large manager with the five stats in one row, progress
  bar, depot progress, and icon-only Pause/Resume and Cancel controls.
- [ ] A stalled live download shows the elapsed no-progress time.
- [ ] Pause freezes the downloader; Resume continues it from the same files.
- [ ] Cancel removes partial content and shows Download again.
- [ ] A dead downloader becomes an error state and a retryable Download button.
- [ ] A start error (for example a missing Lua manifest) shows the error and
  leaves Download available.
- [ ] Closing the manager leaves the download running.
- [ ] Removing a game during its download stops the downloader.
- [ ] A missing local Lua manifest is regenerated when the manifest source is
  available.

## Verification

- `cargo check -p steamtron-desktop` passed.
- `cargo test -p steamtron-desktop --bin steamtron-desktop download::tests`
  passed 4/4: pause sends `SIGSTOP` and resume sends `SIGCONT` to the process
  group; cancel terminates a paused process; a non-terminal state without a
  process, or with a dead PID, becomes `error`.
- JSDOM download-manager harness passed: Downloading opened the large manager,
  progress/average speed/ETA/depot progress rendered, Pause/Resume icon controls
  invoked their commands, closing kept the download, and Cancel showed the
  cancelled state before the top-right close restored the Download button.
- Rust test `status_reports_a_live_download_with_no_recent_output_as_stalled`
  passed: a live process with 30s of no output reports 29+ stalled seconds.
- Existing navigation suites still pass.

## Decisions and risks

- Pause/Resume uses process signals rather than an SMD protocol command. It
  depends on the Unix process group created for the child. On non-Unix platforms
  Pause/Resume returns an explicit unsupported error; Cancel remains available.
- A paused SMD process holds its network connections frozen; long pauses can let
  the CDN connection time out. Resume relies on SMD's chunk retries.
- The broad modal restores the pre-`0101ca2` download window, extended with
  pause/resume, average speed, ETA, depot progress, and stalled reporting.
  UmuTron's `open_download_manager` supplied the data/install bar split and
  stalled-stage behavior. The old window was removed when downloads became
  inline; both now coexist: inline progress stays on the game card and the
  Downloading button reopens the full manager.
- Queue/up-next is not implemented because SteamTron currently runs one
  download per game rather than a shared global queue.
- Cancelling removes the managed game root, matching the existing failed-download
  cleanup. SMD checkpoints in its cache are left in place.
- Removing a game entry now cancels an active download first, so the downloader
  cannot keep writing after the entry is gone.
- A missing Lua manifest is regenerated from the signed-in API; if that fails the
  user still gets the explicit re-add error.

## Links

- Related PRD: `docs/prd/0022-smd-downloader.md`
- Implementation: `crates/steamtron-desktop/src/download.rs`,
  `crates/steamtron-desktop/ui/app.js`
