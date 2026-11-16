# Install Windows build under Program Files (x86)

- Status: active
- Date: 2026-11-16
- Areas: `crates/steamtron-desktop/tauri.conf.json`, `windows/installer-hooks.nsh`

## Problem

The NSIS installer uses `currentUser` mode, so SteamTron is installed under
`%LOCALAPPDATA%\SteamTron`. The requested install location is
`C:\Program Files (x86)\SteamTron`.

## Requirements

- R1: Use per-machine installation with UAC elevation.
- R2: Force the x64 app into `$PROGRAMFILES32\SteamTron`.
- R3: Keep the install directory as `$INSTDIR` for shortcuts, uninstaller and
  registry entries.
- R4: Do not change application data; settings remain per-user.

## Acceptance criteria

- [ ] Clean install lands in `C:\Program Files (x86)\SteamTron`.
- [ ] Start-menu shortcut launches the installed binary.
- [ ] Uninstaller removes the Program Files directory.
- [ ] App data remains under the user profile.

## Verification

- Windows VM install, `dir "C:\Program Files (x86)\SteamTron"`, launch, uninstall.
