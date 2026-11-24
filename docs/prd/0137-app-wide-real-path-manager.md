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
  next Play or library snapshot when no managed run owns it.
- R9: No direct overwrite of the installed game files.
- R10: Uninstall and move restore the real-path layout first, so they never
  delete or move only a junction and strand the hidden base.
- R11: A library snapshot taken while Play is swapping the junction counts the
  recorded base as installed content, so the UI does not flash Download for an
  installed game.
- R12: Restoring a game backup prepares the target root first: a valid layout is
  put back to its base, and a stale junction is removed, before the commit
  rename.
- R13: Restore keeps an existing configured launcher whose executable still
  exists. The backup's targets are fallback, and auto-detection is the final
  fallback; a stale snapshot must not replace a patched launcher.

## Acceptance criteria

- [x] Launcher detection covers Uplay, SocialClub, RGL and EA/Origin tokens.
- [x] Play auto-prepares a real-path layout for a game with launcher fixes and
      reuses an existing layout record.
- [x] Simple games still use the drive-letter overlay.
- [x] Max Payne 3 still uses its existing layout record.
- [x] Far Cry 5 Play auto-prepares and launches with the real path from a normal
      non-elevated app (0.1.60; PRD 0138).
- [ ] Watch Dogs 2 still launches with the real path.
- [ ] Restore moves the base back and removes the junction/mount/record.
- [ ] Uninstall and move restore the layout first.
- [x] Backup restore handles a real-path junction at the target root.
- [x] Restore keeps an existing configured launcher when its executable still
      exists; backup targets are fallback.
- [ ] Forced-exit repair is exercised.

## Verification

- 0.1.57: Max Payne 3 desktop Play with the manual layout record (PRD 0135).
- 0.1.58 installed from the published installer (SHA-256
  `609dd001e470c22a18f8cf9511226433c9cde19ae83e39ddc7ee962f7969b94f`).
- Far Cry 5 Play created `.steamtron-base/Far Cry 5`,
  `.steamtron-mount/Far Cry 5`, the real-path junction and
  `overlays/af1b96af-...json` automatically.
- 0.1.59 with the app running elevated and GoldBerg applied: Far Cry 5 launched
  through the real path (game PID 6428, overlay wrapper PID 17088) and the log
  `play-1795431669.log` shows the mount, junction, cwd and target.
- After that Play the UI briefly showed Download for the running game because
  the library snapshot ran while the junction was being swapped or left on the
  unmounted mount. 0.1.60 counts the recorded base as installed (R11) and
  repairs stale junctions on refresh; code verified on Linux only, not yet
  exercised in an installed build.
- 0.1.60 with normal non-elevated SteamTron: the overlay wrapper logged the
  elevation relaunch and started Far Cry 5. The user confirmed it played.
- 0.1.62 installed. FC5 Play from the normal app prepared the real path, logged
  the elevation relaunch, started the elevated wrapper and left the game running
  (PID 12720, `play-1795479065.log`).
- Watch Dogs 2 also launched through the catalog path (`bin/WatchDogs2.exe`,
  wrapper PID 7652); the user confirmed it played.
- 0.1.62 GTA V Play is blocked by the catalog `requires_patch` entry before any
  overlay or process tree starts.
- Max Payne 3 was not installed: `.maxpayne3-base` and `.maxpayne3-mount` were
  missing, and the real path was a stale junction.
- Restoring the Max Payne backup on 0.1.62 extracted 32.6 GB then failed with
  `Access is denied (os error 5)` at the commit rename, because the target was
  still a junction. The stale junction and sidecar were removed and the retry
  succeeded.
- The retry replaced the configured PlayMaxPayne3 target with MaxPayne3, so the
  game asked for the Rockstar launcher. Setting the primary target to
  `Max Payne 3\PlayMaxPayne3.exe` made the game run (PID 6992), without any
  game-specific catalog target. 0.1.63 keeps existing targets that still exist
  and treats backup targets as fallback; not yet installed.
- Not run yet: restore on 0.1.63, uninstall or move after 0.1.58. Only code
  inspection on Linux; Windows-only code is not compiled locally.

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
