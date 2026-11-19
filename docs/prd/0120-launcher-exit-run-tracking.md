# Managed run tracking survives a launcher exit on Windows

- Status: active
- Date: 2026-11-19
- Areas: `crates/steamtron-desktop/src/main.rs`,
  `crates/steamtron-desktop/src/process_tree.rs`,
  `crates/steamtron-desktop/ui/app.js` (unchanged; polling only)

## Problem

The Play button flips back to "Play" while the game is still running when the
game's launcher exits after starting the real executable.

Witcher 3 (appid 292030) has no fixes, so Play takes the direct native path and
spawns the configured launcher. Its own log in
`%LOCALAPPDATA%\steamtron\diagnostics\292030\play-1795095903.log` shows:

```
19:15:03.855 [red-prelauncher] Reading ... launcher-configuration.json
19:15:03.856 [red-prelauncher] Skipping REDlauncher due to commandline options.
19:15:03.856 [red-prelauncher] Requested to launch the game.
19:15:03.969 [red-prelauncher] The game was launched under PID 16420.
```

The launcher exits, `witcher3.exe` (pid 16420) keeps running, and the run
marker is removed the moment the direct child exits:

```rust
let _ = child.wait();
if let Ok(mut runs) = managed_runs().lock() {
    if runs.get(&tracked) == Some(&pid) {
        runs.remove(&tracked);
    }
}
```

The UI polls `running_managed()`, so it renders Play again as soon as the
20-second local grace expires. Stop would also fail, because
`stop_managed_blocking` uses `taskkill /PID <launcher pid> /T`, and that pid no
longer exists.

The overlay path does not have this problem: the overlay wrapper waits for the
job's whole tree (0117). Only the direct native path lacks tree tracking.

## Goal

On Windows, a managed game stays marked as running until every process started
by its launcher is gone, and Stop terminates that whole tree.

## Non-goals

- Changing Linux tracking (Proton's `waitforexitandrun` already stays alive).
- Changing the Play button polling intervals or the 20-second grace.
- Persisting the running state across a SteamTron restart; that remains
  in-memory.

## Requirements

- R1: For every managed Windows launch (direct or overlay), assign the spawned
  child to a job object created by the app.
- R2: Keep the `managed_runs` entry until the child and every descendant in the
  job have exited, not just until the direct child exits.
- R3: `stop_managed` terminates the job (all descendants) when available and
  falls back to `taskkill /T` when job tracking is unavailable.
- R4: If the job cannot be created or the assignment fails, log it and keep the
  current direct-child behaviour instead of failing the launch.
- R5: The overlay wrapper keeps its own job; nested jobs are expected and must
  not break the union lifetime.

## Acceptance criteria

- [ ] Start a launcher-based game (Witcher 3/REDprelauncher) through Play; the
      UI keeps showing Stop after the launcher exits while the game runs.
- [ ] Stop from that state terminates the game process and returns the UI to
      Play.
- [ ] Starting a game, letting it exit naturally, makes the UI return to Play
      without a manual Stop.
- [ ] A game with an overlay (RDR2) still launches, stops, and unmounts as
      before.
- [ ] When the app is restarted, a game started by the previous instance is not
      tracked (known limitation, unchanged).

## Verification

Pending. Reproduced with Witcher 3 at 19:15:03: launcher exited, `witcher3.exe`
pid 16420 stayed alive, UI showed Play. Runtime re-check after the fix will use
the same game and record the process/UI state.

## Decisions and risks

- A job object is the only reliable way to follow descendants after the parent
  exits on Windows; enumerating children after the fact loses the link.
- `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` keeps the Stop semantics: closing the job
  handle kills the tree.
- Risk: a descendant that outlives the game (for example a vendor service the
  launcher starts) would keep the game marked as running. If observed, the
  follow-up is to track the game executable as the primary member rather than
  every job member.
- Risk: nested-job assignment can fail inside restricted environments; R4 keeps
  the launch working and only degrades tracking.

## Links

- `0117-fixes-launcher-overlay-lifetime.md` (overlay wrapper job tracking).
- Witcher 3 appid 292030, game id `292030`.
