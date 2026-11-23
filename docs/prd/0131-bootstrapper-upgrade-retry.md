# Windows bootstrapper retries the inner installer after an upgrade

- Status: active
- Date: 2026-11-23
- Areas: `steamtron-desktop/windows/bootstrapper.nsi`

## Problem

On an upgrade, the inner Tauri NSIS installer's first pass uninstalls the
previous SteamTron and exits after that step. The bootstrapper treated exit 0
as success and finished without the app installed.

Observed upgrading 0.1.46 → 0.1.49 on 2026-11-23: the wizard removed 0.1.46,
left only `winfsp-x64.dll` in `C:\Program Files (x86)\SteamTron`, and no
`steamtron-desktop.exe`; the registry uninstall entry was gone. The same
behavior explains the earlier silent `/S` failure.

## Goal

One setup run completes an upgrade. Fresh installs keep working.

## Requirements

- R1: After the inner installer exits 0, check
  `$INSTDIR\steamtron-desktop.exe`.
- R2: If missing, wait for the old uninstaller to release locks and run the
  inner installer again.
- R3: If the second run fails or the exe is still missing, abort with a clear
  message instead of reporting success.
- R4: Applies to both the interactive wizard and `/S`.
- R5: No change to the prerequisite checks, install directory, or fresh
  installs.

## Acceptance criteria

- [ ] Install 0.1.49, then run the 0.1.52 setup once; it ends with 0.1.52
      installed.
- [ ] A fresh install still installs 0.1.52 in one run.
- [ ] Silent `/S` upgrades install instead of leaving no app.

## Verification

- Code: `bootstrapper.nsi` checks the installed exe and retries the embedded
  installer.
- Build: the Release workflow compiles the NSIS bootstrapper on its Windows
  runner.
- Not run yet: the 0.1.49 → 0.1.52 upgrade test (requires 0.1.49 installed).

## Decisions and risks

- The inner installer's uninstall-first behavior is Tauri's, so the wrapper
  handles it instead of changing the generated installer.
- A single retry is used. If a future Tauri version needs more, the failure is
  explicit and visible rather than a silent no-op.
- The check uses `$INSTDIR\steamtron-desktop.exe`, the same per-machine path the
  bootstrapper sets.
