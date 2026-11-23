# Launcher restart-loop watchdog

- Status: active
- Date: 2026-11-23
- Areas: `steamtron-desktop` Windows overlay wrapper

## Problem

A broken game-specific launcher can respawn itself forever. GTA V Enhanced's
RUNE pack did:

`PlayGTAV.exe -> GTA5_Enhanced.exe -> PlayGTAV.exe -> ...`

The overlay wrapper kept the junction and mount open waiting for the process
tree, so the app stayed on Stop and the user saw no game.

## Goal

Detect a rapid launcher respawn loop, stop the tree, restore the junction and
unmount, and return a clear error instead of waiting forever.

## Requirements

- R1: Track total processes created in the job, not only active processes.
- R2: After the initial launcher exits, count new processes over a 15-second
  window.
- R3: If the tree creates 60 or more processes in that window, terminate it.
- R4: Restore the junction, unmount, and return a clear error.
- R5: Do not trigger for a game that keeps its initial process alive.
- R6: No game file changes.

## Acceptance criteria

- [ ] The GTA V loop is stopped within roughly 15 seconds when it is launched
  without the catalog guard.
- [ ] A normal launcher that spawns a small child tree is unaffected.
- [ ] Failure restores the real path and removes the mount.

## Verification

- 0.1.62 code: `ProcessTree::total` and the wrapper watchdog.
- Not yet run in an installed build.

## Decisions and risks

- The threshold is intentionally high to avoid killing normal games that spawn
  worker processes. A game-specific catalog entry remains the first line of
  defence.
- Job accounting is available on Windows; non-Windows paths are unchanged.

## Links

- PRD 0136 (catalog and modules)
- PRD 0137 (real-path overlay)
