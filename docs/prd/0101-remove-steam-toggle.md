# Remove the per-game Steam-running toggle

- Status: done
- Date: 2026-11-18
- Areas: `crates/steamtron-desktop/ui/app.js`, `crates/steamtron-desktop/src/main.rs`

## Problem

Game Settings showed a "Require Steam to be running" switch in the
Compatibility pane on both platforms. It duplicated knowledge the user already
has (Steamworks games need Steam) and its save button wrote a hidden launch
flag that is easy to leave on.

## Goal

The toggle, its save button, and the write-only IPC parameter are gone on Linux
and Windows. Existing libraries keep their stored flag, but the UI no longer
offers or implies the setting.

## Requirements

- R1: Remove the toggle and "Save Steam requirement" button from both
  compatibility panes.
- R2: Remove the `requires_steam` parameter and its write block from
  `update_managed_launch_settings`.
- R3: Do not change launch behavior for libraries that already stored the flag.

## Acceptance criteria

- [x] No `#gs-steam-required` element or "Require Steam to be running" text
  exists in either Game Settings pane.
- [x] Saving Proton or prefix source still works (the command keeps its other
  parameters).

## Verification

- Diff of `renderCompatibilityTab` in `ui/app.js`: only the Steam-client
  heading, switch, save button, and handler are removed; the Proton and prefix
  controls remain.
- JS syntax check passes; the Rust command compiles after dropping the
  `requires_steam` parameter.
- The Compatibility tab is hidden on Windows, so the live Windows build cannot
  show the removed section; the Linux pane is covered by the same diff.

## Decisions and risks

- The library field and the launch gate remain for imported libraries; removing
  them would silently change how an existing `requires_steam = true` game
  starts.
- The update command loses a parameter that no caller sends anymore; Tauri
  commands are internal, so this is not a public API break.

## Links

- Related: `0096-windows-union-overlay`.
