# App-wide real-path overlay manager

- Status: active
- Date: 2026-11-23
- Areas: `steamtron-desktop` managed Play, Windows overlay

## Problem

0.1.57 can launch a game through a real-path WinFsp union, but only when a
per-game sidecar already exists. Max Payne 3 was prepared manually; Far Cry 5
had no sidecar and fell back to the drive-letter overlay, which Ubisoft
launchers reject. Requiring a manual sidecar per game defeats the app-wide
“click Play” goal.

## Goal

On Play, the app decides automatically whether a game needs the real-path
overlay. If its fixes contain a launcher module (Uplay/SocialClub/RGL/EA), it
prepares the layout itself: base on the same volume, mountpoint, real-path
junction and internal layout record. Simple games keep the drive-letter
overlay. The user never creates a sidecar.

## Requirements

- R1: Detect launcher fixes from the fixes tree (`uplay_r1_loader`,
  `uplay_r2_loader`, `socialclub`, `launcher/launc`, `gsrld`, Origin/EA
  modules) and RGL `.rkstr`/`.tbm` in the launch target.
- R2: If a layout record exists, reuse it and repair the real-path junction to
  the pristine base before Play.
- R3: If no record and the real folder is a normal directory, move it to
  `<parent>/.steamtron-base/<folder>` on the same volume (no copy), create the
  mountpoint and the junction, and write `overlays/<game-id>.json`. If the
  junction cannot be created, move the content back to the original path.
- R4: If the real path is already a junction (existing manual layout), adopt its
  base and create the record instead of moving it again.
- R5: Simple games without launcher fixes keep the drive-letter overlay.
- R6: Preserve Import/Export/Remove fixes; the layout manager is additive and
  never deletes user fixes.
- R7: A prepared real-path layout exposes a restore action in the Fixes pane
  that moves the base back and removes the junction/mount/record.
- R8: A stale junction left by a forced exit is repaired to the base on the
  next Play.
- R9: No direct overwrite of the installed game files.
- R10: Uninstall and move restore the real-path layout first, so they never
  delete or move only a junction and strand the hidden base.

## Acceptance criteria

- [x] Launcher detection covers Uplay, SocialClub, RGL and EA/Origin tokens.
- [x] Play auto-prepares a real-path layout for a game with launcher fixes and
      reuses an existing layout record.
- [x] Simple games still use the drive-letter overlay.
- [x] Max Payne 3 still uses its existing layout record.
- [ ] Far Cry 5 Play auto-prepares and launches with the real path.
- [ ] Watch Dogs 2 still launches with the real path.
- [ ] Restore moves the base back and removes the junction/mount/record.
- [ ] Uninstall and move restore the layout first.
- [ ] Forced-exit repair is exercised.

## Verification

- 0.1.57: Max Payne 3 desktop Play with the manual layout record (PRD 0135).
- 0.1.58 code: `ensure_real_path` / `needs_real_path`, auto-called from
  `play_managed`; `restore_managed_real_path` command and Fixes-pane action;
  uninstall and move call the restore helper first.
- Not run yet: installed 0.1.58 Far Cry 5, Watch Dogs 2, restore, uninstall or
  move. Only code inspection on Linux; Windows-only code is not compiled
  locally.
- No runtime evidence for 0.1.58 yet.

## Decisions and risks

- The base move is a same-volume rename; no game data is duplicated.
- The internal layout record is per game because the paths are per game; the
  **feature** is app-wide and automatic.
- A launcher game whose recorded base is missing is not auto-moved again until
  the stale record is cleared. The restore action can clear it once the real
  path is a normal folder again; a stale junction to a missing base needs
  manual cleanup.
- Move only when the real path is a plain directory; adopting an existing
  junction is preferred to avoid breaking manual layouts.
- No game files are modified; everything stays in base/fixes/overlay.

## Links

- PRD 0135 (real-path WinFsp overlay)
- PRD 0136 (launcher-bypass registry)
