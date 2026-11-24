# Non-elevated GUI launch

- Status: active
- Date: 2026-11-24
- Areas: `steamtron-desktop` startup, installer Finish page

## Problem

The Windows installer runs elevated. Its Finish page starts
`steamtron-desktop.exe` with the installer's elevated token, so the whole GUI
and tray run as administrator. This causes:

- a second instance at a different integrity level when the user later starts
  the normal shortcut;
- single-instance messaging that may not cross the elevation boundary;
- unnecessary administrator privileges for the UI;
- inconsistent `%TEMP%`, process ownership and UAC behavior.

The app only needs elevation for specific per-game overlay/runtime children.

## Goal

The GUI always runs as the signed-in user. An elevated GUI hands off to a
non-elevated instance and exits. Overlay wrappers and runtime installers keep
their existing elevation.

## Requirements

- R1: Detect the current process token's elevation.
- R2: An elevated GUI relaunches the current executable through `explorer.exe`
  and exits before building the Tauri window/tray.
- R3: A temporary marker prevents a relaunch loop if Explorer cannot drop the
  token; after the marker window, the app continues elevated rather than
  spawning repeatedly.
- R4: `--steamtron-overlay` and runtime child modes are handled before the
  relaunch check and keep their elevation.
- R5: A non-elevated start removes the marker.

## Acceptance criteria

- [x] Elevated GUI detection and Explorer relaunch are implemented.
- [x] Overlay/runtime child dispatch happens first.
- [ ] Runtime: installer Finish page ends with a non-elevated process and one
      tray icon.
- [ ] Runtime: per-game overlay still works after the relaunch.

## Verification

- `cargo check --workspace` passes.
- Windows build must compile the `windows-sys` token calls.
- Runtime verification is pending until 0.1.70 is installed.

## Decisions and risks

- Explorer is the signed-in shell and launches the app at the normal
  integrity level. If it cannot, the marker allows the app to continue
  elevated instead of looping.
- Users who intentionally start the GUI as administrator lose that elevation;
  game/runtime operations still elevate on demand.
- The installer Finish page still launches an elevated process briefly; it
  exits immediately after handing off.

## Links

- `docs/prd/0143-single-instance-tray.md`
