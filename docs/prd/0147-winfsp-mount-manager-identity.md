# Mount Manager volume identity for real-path overlays

- Status: active
- Date: 2026-11-24
- Areas: `steamtron-overlay` Windows backend, desktop overlay launcher, Windows bootstrapper

## Problem

Grand Theft Auto V Enhanced starts when the RUNE fix is copied directly into
the game folder on NTFS, but its launcher loops under the WinFsp real-path
overlay: `PlayGTAV.exe -> GTA5_Enhanced.exe -> PlayGTAV.exe` repeatedly, with
no game window.

Evidence gathered on 2026-11-24:

- Overlay at `.steamtron-mount/<Game>` plus junction: 60+ processes, no window.
- Overlay mounted directly at the real game path: 22 processes, no window.
- Same files on NTFS (read-only hardlink staging): 2 processes and a
  `Grand Theft Auto V` window.
- `GTA5_Enhanced.exe` imports `GetFinalPathNameByHandleW` and
  `GetVolumeInformationA`.
- Through the legacy WinFsp mount, `GetFinalPathNameByHandleW` fails with
  error 1005 (`ERROR_UNRECOGNIZED_VOLUME`), `GetVolumePathNameW` returns a
  `\\?\GLOBALROOT\Device\Volume{…}` path, and file IDs are zero.
- WinFsp only registers a volume with the Windows Mount Manager when the mount
  point uses `\\.\…` syntax. A normal `S:` or directory mount uses
  `DefineDosDevice`/a junction and is invisible to the Mount Manager; user-mode
  Mount Manager access requires elevation.
- Mounting with `\\.\…` while elevated makes
  `GetFinalPathNameByHandleW` return the real path; launching from the excluded
  SteamTron wrapper then produced 2 processes and the game window.
- With WinFsp's `MountUseMountmgrFromFSD=1`, the same Mount Manager mount works
  from a **non-elevated** wrapper: 2 processes, game window, no UAC.

The base files stayed hash-identical after every test, and the overlay upper
layer was empty, so Mount Manager does not dirty `.steamtron-base`.

## Goal

Every real-path overlay mount uses the Windows Mount Manager when available,
so games and launchers see a real volume/path identity, while normal Play
remains non-elevated and the overlay layout is unchanged.

## Non-goals

- Do not revert to the old `S:\`-only architecture.
- Do not change `.steamtron-base` / `.steamtron-mount` / junction layout.
- Do not add game-specific patches, targets, or catalog overrides.
- Do not modify installed game files.

## Requirements

- R1: The Windows overlay tries a Mount Manager mount (`\\.\…`) for every
  explicit directory mount and every free-drive mount.
- R2: If the Mount Manager mount fails, fall back to the current
  `DefineDosDevice`/junction mount so existing games do not regress.
- R3: The Windows bootstrapper sets WinFsp's
  `HKLM\SOFTWARE\WOW6432Node\WinFsp\MountUseMountmgrFromFSD` to `1` when the
  value is absent, enabling non-elevated Mount Manager mounts. Respect an
  existing value.
- R4: The overlay log records whether the mount used the Mount Manager or the
  legacy path.
- R5: Overlay writes continue to copy up into the upper layer; the base stays
  pristine.
- R6: The non-elevated GUI and the normal Play flow remain non-elevated.
- R7: When a real-path layout is present, mount the union at the game's real
  folder for the run, not at `.steamtron-mount`; the real path is restored as a
  junction to `.steamtron-base` after unmount.
- R8: Add the game's `.steamtron-base` and real `.steamtron-mount` folders to
  the Defender exclusions so fix executables are not quarantined.
- R9: Remove GTA V Enhanced's obsolete `requires_patch` gate.

## Acceptance criteria

- [ ] A fresh install sets `MountUseMountmgrFromFSD=1` only when absent.
- [ ] GTA V starts from normal Play with 2 processes and the game window.
- [ ] The overlay log reports `mount_mode=mount-manager` and
      `mounted_at=<real path>` for the real-path overlay.
- [ ] Base hashes are unchanged after Play; the upper layer contains any
      game-written overlay files and no base file is modified.
- [ ] Defender logs no new detection for `PlayGTAV.exe`, `version.dll`, or
      `RUNE64.dll` during Play.
- [ ] Far Cry 5, Max Payne 3, Watch Dogs 2, Assassin's Creed Origins and Little
      Nightmares II still launch.
- [ ] If the Mount Manager mount is unavailable, the legacy mount still works.

## Verification

- 2026-11-24 physical NTFS test: 2 processes, window `Grand Theft Auto V`;
  base hashes unchanged.
- 2026-11-24 elevated Mount Manager test: `GetFinalPathNameByHandle` returned
  the real path; a wrapper-launched game produced 2 processes and the window.
- 2026-11-24 non-elevated FSD test (`MountUseMountmgrFromFSD=1`):
  `merged=\\.\D:\…\Grand Theft Auto V Enhanced`, 2 processes, game window,
  base hashes unchanged, upper layer empty, junction restored.
- 2026-11-24 0.1.71 normal Play still mounted at `.steamtron-mount` + junction
  and looped (`mount_mode=mount-manager`, 8+ PlayGTAV processes). Defender
  detected `PLAYGTAV.EXE`, `VERSION.DLL` and `RUNE64.DLL` under
  `.steamtron-mount`; adding that folder and `.steamtron-base` to the
  exclusions stopped new detections but the loop remained.
- 2026-11-24 direct real-path Mount Manager tests (elevated and non-elevated
  FSD) both produced 2 processes and the game window. This established that
  Mount Manager alone was not enough; the union must also be mounted at the
  real path.
- Code and installer verification are pending the 0.1.72 build.

## Decisions and risks

- Mount Manager fixes volume/path identity only; it does not make `S:\` look
  like the real install path. The real-path layout remains required for RGL,
  Uplay and other path-sensitive launchers.
- Mount Manager at `.steamtron-mount` still exposes that path to
  `GetFinalPathNameByHandle`. RUNE compares it with the real install path, so
  real-path layouts mount at the real folder for the run.
- `.steamtron-mount` remains on disk as the legacy fallback; Stop/unmount
  restores the real-path junction to `.steamtron-base`.
- `MountUseMountmgrFromFSD` is a global WinFsp setting and WinFsp documents it
  as not recommended for general use. It is set only when absent, and the
  overlay falls back to the legacy mount when it is unavailable.
- The Mount Manager directory mount must not pre-exist; the overlay removes an
  empty mount directory before the attempt.
- Defender process exclusions remain required for fix executables. The normal
  Play flow applies them before spawning the overlay.

## Links

- PRD 0135 (real-path WinFsp overlay)
- PRD 0136 (launcher bypass registry)
- PRD 0138 (elevated game launchers)
- PRD 0140 (launcher loop watchdog)
- PRD 0146 (non-elevated GUI)
