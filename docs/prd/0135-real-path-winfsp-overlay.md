# Real-path WinFsp overlay via a local directory mount and junction

- Status: active
- Date: 2026-11-23
- Areas: `steamtron-overlay` Windows backend, `steamtron-desktop` overlay launcher

## Problem

The existing Windows overlay mounts the union of the game and fixes on a free
drive letter (`S:`). Games with plain Steamworks (Watch Dogs 2) work, but
Rockstar RGL, Ubisoft Connect and EA/Origin launchers reject the unrelated
drive-letter path: they validate the real install path, load their own DLLs
from the real folder, and use anti-tamper/raw file access.

Directly overwriting the game folder makes those launchers work, but it
violates the product rule that the installed game files stay pure.

A junction directly to the WinFsp drive does not work either. Tested on
Windows 11:

- `mklink /J <path> S:\` fails with “Local volumes are required to complete
  the operation.”
- `mklink /D <path> S:\` fails without administrator/Developer Mode.

## Goal

Keep WinFsp and the fixes overlay, but present the game's **real install path**
to the launcher. The original files stay in an untouched base layer; no game
data is copied or overwritten.

## Requirements

- R1: Allow the Windows overlay to mount the union at an empty **local
  directory** instead of a drive letter.
- R2: Store the original game folder as a pristine **base** on the same
  volume; the real game path becomes a junction.
- R3: Normally the junction points at the base so downloads, backups,
  verification and the library keep working. During Play it points at the
  WinFsp directory mount so the game sees the real path.
- R4: Retarget the junction atomically around the overlay process, including
  launchers that exit and leave child processes running.
- R5: Restore the junction to the base on every exit/error path.
- R6: Present a local NTFS-like volume: filesystem name `NTFS`, stable volume
  serial, case-insensitive lookup, preserved case, Unicode names.
- R7: No file copies of the game, no direct overwrite of the original folder.
- R8: If the directory mount or launcher still fails, surface the error
  clearly; do not silently fall back to modifying game files.
- R9: Managed Play reads a per-game real-path layout sidecar
  (`<data home>/overlays/<game-id>.json` with real/base/mount) and passes
  `--mountpoint`, `--junction` and `--base` to the overlay. The desktop Play
  button therefore uses the proven real-path flow. Preparing and removing the
  layout from the UI is a follow-up.

## Acceptance criteria

- [x] `mklink /J` to the WinFsp drive is proven impossible; the design does not
      rely on it.
- [x] Windows overlay accepts a directory mountpoint and uses it as the merged
      path.
- [x] Overlay launcher accepts `--mountpoint`, `--junction`, `--base`, creates
      the junction after mount, and restores it before unmount.
- [x] Max Payne 3 launches through the real path with the Rockstar fix and no
      “Rockstar required” message.
- [ ] Desktop Play starts Max Payne 3 through the sidecar-driven real-path flow.
- [ ] The UI can prepare/remove the real-path layout for other games.
- [ ] A launcher that spawns child processes keeps the junction/mount alive
      until the whole tree exits.
- [ ] Downloads/backups work while the junction points at the base.
- [ ] A forced overlay exit restores or repairable-resets the junction.

## Verification

- Tested: WinFsp drive `S:` mounts.
- Tested: `mklink /J` to `S:` fails with “Local volumes are required.”
- Tested: `mklink /D` to `S:` fails without elevation.
- Code: Windows backend gained `with_mount_point` and a directory mount path.
- Code: overlay CLI gained `--mountpoint`, `--junction`, `--base`.
- Installed 0.1.54: the directory mount failed with
  `STATUS_OBJECT_NAME_COLLISION (0xD0000035)` because the empty mountpoint
  already existed; the junction stayed on the base and no game launched.
- 0.1.55: removes/recreates the mountpoint and retries, then tries the extended
  path form.
- Installed 0.1.55 on Windows 11: manual
  `--mountpoint/.maxpayne3-mount --junction/…/Max Payne 3 --base/.maxpayne3-base`
  launch. The game started through the real path with the injected fixes; the
  user confirmed “game started hurray.” The junction was still pointing at the
  base before the launch and was retargeted to the mountpoint for the game.

## Decisions and risks

- Junctions require a local target, so the WinFsp mountpoint must be a local
  directory, not the virtual drive.
- The base move is a same-volume rename; no extra game data is stored.
- The junction is a reparse point. A launcher may resolve it and still reject
  the virtual filesystem; the runtime test decides.
- If the launcher rejects the local junction/WinFsp view, the remaining
  non-destructive fallbacks are a symlink with elevation or a kernel
  minifilter; direct overwrite is rejected.
- The app must repair a stale junction after a crash; the guard restores on
  normal errors, but a forced kill can leave it pointing at an unmounted
  mountpoint.

## Links

- PRD 0131/0134 (installer)
- PRD 0132 (bundled runtimes, done)
