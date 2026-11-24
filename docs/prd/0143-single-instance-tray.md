# Single SteamTron instance and tray icon

- Status: active
- Date: 2026-11-24
- Areas: `steamtron-desktop` startup, tray, installer finish action

## Problem

Each installer “Launch SteamTron” action, desktop shortcut or second launch
starts another `steamtron-desktop.exe`. Each instance creates its own tray
icon, so the tray fills with duplicate SteamTron icons and commands may run in
an older/elevated instance instead of the window the user is looking at.

The installer’s elevated Finish page launches the app with the installer’s
elevated token. That is also why some instances run elevated while a later
shortcut starts a non-elevated one.

## Goal

One SteamTron GUI instance per user session. A second launch focuses the
existing window and exits; only one tray icon exists.

## Non-goals

- Do not drop elevation for the per-game overlay wrapper or runtime installer
  children. Those keep their current elevation behavior.
- Do not change how the app elevates a game launcher.

## Requirements

- R1: Register `tauri-plugin-single-instance` before the app setup.
- R2: On a second launch, show, unminimize and focus the existing `main`
  window, then exit the second process.
- R3: The second process must not reach `setup`/tray creation.

## Acceptance criteria

- [x] The single-instance plugin is registered first in the Tauri builder.
- [x] The callback restores and focuses the main window.
- [x] Runtime: launching the installed app twice leaves one process and one
      tray icon (0.1.68, second launch left process 5532 only).

## Verification

- `cargo check --workspace` — exit 0 (0.1.66).
- 0.1.68 installed; launching the app a second time left only the first
  process (PID 5532). Runtime verified.

## Decisions and risks

- The plugin is Rust-side only; no frontend permission is required.
- A second launch from a different elevation level may not be able to message
  the first instance; it exits rather than creating a duplicate. The installer
  launch path remains a separate concern.

## Links

- `docs/prd/0141-appmanifest-settings-config.md`
