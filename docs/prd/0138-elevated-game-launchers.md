# Elevation-required game launchers

- Status: active
- Date: 2026-11-23
- Areas: `steamtron-desktop` managed Play, Windows overlay wrapper

## Problem

Some fix launchers ship an executable whose manifest requests administrator
rights. Normal `CreateProcess` fails with `ERROR_ELEVATION_REQUIRED` (740), so
the overlay wrapper closes the mount and the game never starts.

Observed on 0.1.58 with Far Cry 5 and the imported CODEX/Uplay fix:

```
SteamTron overlay: merged=D:\...\.steamtron-mount\Far Cry 5
SteamTron overlay: junction=D:\...\Far Cry 5 -> D:\...\.steamtron-mount\Far Cry 5
SteamTron overlay: cwd=D:\...\Far Cry 5\bin
SteamTron overlay: exec=D:\...\Far Cry 5\bin/FarCry5.exe
SteamTron overlay wrapper: could not launch game: The requested operation requires elevation. (os error 740)
```

The real-path layout itself worked: the base, mountpoint, junction and sidecar
were created automatically, and the overlay mounted before the failed launch.

## Goal

When a game launcher requires elevation, Play asks Windows for elevation once,
starts the same executable with the same arguments, working directory and
environment, keeps the overlay mounted while the game runs, and reports a clear
error if the user declines.

## Non-goals

- Running all of SteamTron as administrator.
- Bypassing or suppressing the Windows UAC prompt.
- Elevating games that do not require it.
- Changing game files or the real-path layout design.

## Requirements

- R1: Catch `ERROR_ELEVATION_REQUIRED` from the game spawn in both the overlay
  wrapper and the direct managed Play path.
- R2: Relaunch through `Start-Process -Verb RunAs` with the original executable,
  quoted arguments, working directory and environment.
- R3: Wait for the elevated process tree before returning, so the overlay stays
  mounted until the game exits.
- R4: Log that elevation is required and that the UAC prompt is waiting.
- R5: If elevation is declined or fails, return the actual error and let the
  wrapper restore the junction and unmount normally.
- R6: Games that do not require elevation keep the existing direct spawn.
- R7: No game file is modified.

## Acceptance criteria

- [ ] Far Cry 5 Play on a non-elevated SteamTron shows a UAC prompt, then starts
      the game with the real-path overlay mounted.
- [ ] The journal records the elevation-required retry and the eventual exit.
- [ ] Declining the UAC prompt restores the junction and unmounts the overlay.
- [ ] A non-elevated game still starts without a prompt.
- [ ] Direct Play (without fixes) supports the same elevation fallback.

## Verification

- 0.1.58 installed, real-path auto-prepare verified for Far Cry 5; launch failed
  with os error 740 (log above).
- 0.1.59 with the app already elevated and GoldBerg applied: Far Cry 5 launched
  through the real path (game PID 6428, wrapper PID 17088). This proves the game
  and overlay are otherwise correct.
- 0.1.59 with a normal (non-elevated) app: the game spawn failed with 740 and
  the per-game `Start-Process -Verb RunAs` retry failed with
  `The volume does not contain a recognized file system` because its working
  directory was on the WinFsp union. The junction and mount were restored.
- 0.1.60 code changes the retry design: the wrapper restores the junction,
  unmounts, then relaunches **itself** elevated with the same args and a normal
  working directory. The elevated wrapper mounts and starts the game directly.
  `--log` keeps diagnostics across the relaunch, and
  `STEAMTRON_OVERLAY_ELEVATED=1` prevents recursion.
- Not yet installed or run; Windows-only code is not compiled locally.

## Decisions and risks

- The first design (elevate the game after the union was mounted) does not work:
  the elevated launcher cannot use a working directory on the WinFsp union.
  Elevating the whole wrapper before the mount avoids that boundary.
- Use PowerShell `Start-Process -Verb RunAs` rather than raw `ShellExecuteEx`
  because the project already relies on that pattern for runtimes and security,
  and it supports `-Wait` for the process tree.
- The UAC prompt is user consent; it cannot be automated or hidden.
- The medium wrapper waits for the elevated wrapper. The Stop action can stop
  the medium wrapper, but Windows may keep the elevated game alive; closing the
  game remains the fallback.
- The elevated wrapper inherits the wrapper's environment, including any
  game-specific variables set by Play.
- The overlay wrapper logs through `--log`, so an elevated process that cannot
  inherit the app's stdout/stderr still writes diagnostics.

## Links

- PRD 0135 (real-path WinFsp overlay)
- PRD 0137 (app-wide real-path manager)
