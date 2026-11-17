# Restart guard and x64 prerequisite detection

- Status: done
- Date: 2026-11-17
- Areas: `crates/steamtron-desktop/ui/app.js`,
  `crates/steamtron-desktop/windows/bootstrapper.nsi`

## Problem

1. Settings shows an enabled Restart Steam button even when Steam is not
   installed.
2. After uninstalling SteamTron and selecting “Delete app data”, the next
   bootstrapper run reports Visual C++ as missing while .NET and WinFsp remain
   installed.

The Windows bootstrapper is a 32-bit NSIS process. It read the x64 VC++ registry
key through the 32-bit view and checked `$SYSDIR`, which redirects to SysWOW64.
The x64 VC++ runtime lives in the native registry view and System32, so a
correctly installed runtime was reported missing.

Tauri's “Delete app data” section removes only `%APPDATA%\<bundle-id>` and
`%LOCALAPPDATA%\<bundle-id>`; it does not uninstall VC++, .NET, or WinFsp.

## Goal

- Restart Steam is disabled when Steam is absent.
- VC++ detection uses the native x64 registry view and System32 DLLs.
- Uninstall data deletion leaves installed prerequisites alone.

## Requirements

- R1: Settings reads Doctor's Steam check and disables Restart Steam when Steam
  is missing.
- R2: The disabled button explains that Steam is not installed.
- R3: Bootstrapper VC++ detection uses `SetRegView 64`.
- R4: VC++ DLL checks use `$WINDIR\Sysnative` from the 32-bit bootstrapper.
- R5: No prerequisite is removed by SteamTron's uninstaller.

## Acceptance criteria

- [x] Restart Steam is disabled without Steam.
- [x] VC++ x64 detection no longer uses the 32-bit registry view.
- [x] VC++ x64 detection no longer checks SysWOW64.
- [x] VC++ x64 detection uses Sysnative, not redirected System32.
- [x] .NET and WinFsp detection remain unchanged.
- [x] Uninstaller behavior is unchanged for prerequisites.

## Verification

- `node --check crates/steamtron-desktop/ui/app.js`
- `cargo check -p steamtron-desktop`
- 0.1.22 and 0.1.24 Windows/Linux release builds succeeded.
- 0.1.24 fixes WOW64 filesystem redirection via `Sysnative`.
- Windows installer SHA-256:
  `9c438fc1f4dfe6ddc29933545b36376c5ef5cbc9cc4cb8cb923c75f49c01fe12`.
- Windows VM interaction is still required to confirm the disabled button and
  prerequisite page visually.

## Decisions and risks

- The report of VC++ being uninstalled was treated as a detection bug because
  the Tauri uninstaller has no VC++ uninstall action. If a runtime is genuinely
  missing, the bootstrapper still installs it.
- `SetRegView 64` is process-wide for the bootstrapper, which only targets x64
  Windows.

## Links

- `crates/steamtron-desktop/windows/bootstrapper.nsi`
- `crates/steamtron-desktop/ui/app.js`
