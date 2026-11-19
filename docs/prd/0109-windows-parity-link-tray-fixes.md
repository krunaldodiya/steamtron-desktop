# Windows parity: link tabs, external fixes, close to tray

- Status: active
- Date: 2026-11-19
- Areas: `crates/steamtron-desktop/ui/app.js`, `crates/steamtron-desktop/src/main.rs`,
  `crates/steamtron-core/src/library.rs`, `crates/steamtron-library/src/managed.rs`

## Problem

1. **Game Settings tabs after linking.** The tab list was built once when the
   modal opened. Adding a launcher updated the game to linked but the nav still
   showed only General/Environment/Debug/Game Link until the dialog was
   reopened. Linux behaved the same; the report came from Windows.
2. **Fixes for external games.** `fixes_dir_for` required an ancestor named
   `SteamTronLibrary`, so a linked game under
   `D:\SteamLibrary\steamapps\common\...` failed with
   `game is not inside a library folder` and Fixes was unusable.
3. **Close to tray on Windows.** The tray was Linux-only (ksni) and the
   `CloseRequested` handler was `#[cfg(target_os = "linux")]`, so closing the
   window exited SteamTron on Windows.

## Goal

Windows matches Linux: tabs refresh immediately after linking; linked games
can receive fixes; closing the window hides to a tray icon with Show/Quit.

## Requirements

- R1: Rebuild the settings nav from the refreshed game after any mutation.
- R2: A shared `fixes_dir(game)` uses the managed path when the root is inside
  `SteamTronLibrary`/`UmuLibrary`, otherwise `%LOCALAPPDATA%\steamtron\fixes\<appid>`
  (or the XDG data home on Linux).
- R3: Use that helper from the library, fix tools, and Play overlay resolution.
- R4: Add a Windows tray icon with Show SteamTron and Quit SteamTron.
- R5: Windows `CloseRequested` hides the window instead of exiting.

## Acceptance criteria

- [ ] Linking a launcher shows DLC/Updates/Installed Files/Fixes without
  reopening Game Settings.
- [ ] An external linked game's Fixes tab no longer reports
  `game is not inside a library folder`.
- [ ] Closing the Windows window keeps the process running and hidden; the tray
  can restore or quit it.

## Verification

- `cargo check -p steamtron-desktop` and `node --check` pass.
- Runtime checks against the installed 0.1.32 build recorded below.

## Decisions and risks

- External fixes live in app data, not beside the game, because linked games
  are read-only references and SteamTron does not write into them.
- Windows tray uses Tauri's `tray-icon` feature instead of ksni.
- Closing always hides on both platforms; Quit is explicit from the tray/app.

## Links

- Related: `0096-windows-union-overlay`, `0090-minimal-doctor`.
