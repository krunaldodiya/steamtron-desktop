# Stop game without freezing the window

- Status: done
- Date: 2026-11-15
- Areas: `crates/steamtron-desktop/src/main.rs`

## Problem

`stop_managed` was a synchronous Tauri command. It sends SIGTERM to the game
process tree, polls every 100 ms for up to 8 seconds, then sends SIGKILL and
scans `/proc` again. Because the command ran on the UI thread, the whole window
appeared hung until the wait finished. A race where processes were already gone
also returned "This game is not running."

## Goal

Stopping a managed game never blocks the window. The dialog/button may show its
busy state, but the UI stays responsive.

## Non-goals

- Changing the game launch path.
- Changing the overlay cleanup for legacy Steam games.

## Requirements

- R1: `stop_managed` runs the process-termination work on a blocking thread.
- R2: The graceful wait is shortened to 3 seconds before SIGKILL.
- R3: No-op when no tracked or matching process exists; return success instead
  of an error.
- R4: A short post-SIGKILL wait avoids a false "did not stop" result.
- R5: The legacy Steam-game stop command is also offloaded from the UI thread.

## Acceptance criteria

- [x] Stopping does not block the UI thread.
- [x] Graceful wait is 3 seconds, followed by forced termination.
- [x] A stop request with no running process succeeds.
- [x] Existing stop confirmation and running-state refresh remain intact.
- [x] Both managed and legacy stop commands run off the UI thread.

## Verification

- `cargo check -p steamtron-desktop` passed.
- Both `stop_managed` and `stop_game` are `async` and call
  `spawn_blocking`; the blocking helpers retain the existing termination
  logic.
- Static diff confirms the deadline changed from 8s to 3s and the added 300ms
  post-kill wait.

## Decisions and risks

- Keeping the work in a blocking thread preserves the existing synchronous
  process-tree implementation while removing the UI freeze.
- 3 seconds is enough for normal Wine/Proton shutdown; SIGKILL remains the
  fallback.

## Links

- Related: `docs/prd/0018-steamless-operations.md`
