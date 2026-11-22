# About section in General settings

- Status: active
- Date: 2026-11-22
- Areas: `steamtron-desktop` UI

## Problem

The installed SteamTron version is not discoverable in Settings. General
rendered only a small version paragraph at the bottom that is easy to miss, and
with an empty `app_version` result it showed just `SteamTron `.

## Goal

A clear **About** section at the very bottom of the General tab showing the
installed application version. No new tab and no new navigation entry.

## Non-goals

- Release notes, update checks, or external links.
- A new settings tab.

## Requirements

- R1: The About section lives under Settings → General, after every existing
  section (at the very bottom).
- R2: It shows the version returned by the existing `app_version` command
  (`CARGO_PKG_VERSION`), matching the installed binary.
- R3: The General tab layout, existing rows, and handlers are unchanged.
- R4: No new tab is added.

## Acceptance criteria

- [ ] Settings → General ends with an About section showing `Version <x.y.z>`.
- [ ] The value matches the installed `steamtron-desktop.exe` file version.
- [ ] General still renders and its buttons work.

## Verification

- Code: `renderGeneralSettings` requests `app_version` and renders the About
  section after Diagnostic logs.
- Code: `node --check crates/steamtron-desktop/ui/app.js`.
- Not run yet: installed-build visual check.

## Decisions and risks

- Reuse the existing `app_version` command; no new API surface.
- Replace the faint one-line paragraph instead of duplicating the version.
