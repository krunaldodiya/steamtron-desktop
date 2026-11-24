# Play and Stop pending feedback

- Status: done
- Date: 2026-11-24
- Areas: `steamtron-desktop` UI

## Problem

After the real-path Mount Manager change, Play can take longer while the base
layout is prepared, the volume mounts, and the wrapper starts. The detail page
kept showing **Play** until `runningManaged` updated, then jumped to **Stop**.
Stop behaved the same way in reverse.

The old temporary `busy(button, "Starting…")` state was replaced by the next
polling render, so the feedback disappeared.

The first Play of a game also runs the existing per-game Defender exclusion
apply, which opens a UAC secure-desktop prompt. The desktop switch back to the
app can look like a UI reload; it is a one-time-per-game Windows prompt, not a
webview reload. Later Plays skip it because the security marker exists.

## Goal

The detail-page primary button immediately shows **Starting…** after Play and
**Stopping…** after Stop, and keeps that state until the backend operation
finishes.

## Requirements

- R1: After Play is confirmed, show a disabled **Starting…** button immediately.
- R2: After Stop is confirmed, show a disabled **Stopping…** button immediately.
- R3: Polling renders must not replace the pending label with Play/Stop.
- R4: Clear the pending state on success and on error.
- R5: Apply to every managed game; no per-game logic.

## Acceptance criteria

- [x] Play changes to **Starting…** before the game process appears.
- [x] Stop changes to **Stopping…** and stays there through the 20-second
      restore window.
- [x] The button returns to Play/Stop correctly after the operation.
- [x] No per-game conditions are added.

## Verification

- 0.1.77 installed. UIA test on GTA:
  `start_labels=Starting…`, `stop_labels=Stopping…`, `final_play=True`.
  The polling render no longer flashes back to Play/Stop.

## Links

- PRD 0147 (Mount Manager)
- PRD 0148 (unified base layout)
