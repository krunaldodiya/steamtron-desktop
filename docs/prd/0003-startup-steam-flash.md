# Remove the startup "Steam installation was not found" flash

- Status: draft
- Date: 2026-11-15
- Areas: `crates/steamtron-desktop/ui/app.js`, `crates/steamtron-desktop/src/main.rs`

## Problem

On a fresh launch the Library paints a red `Steam installation was not found`
message for a few seconds. The managed library then replaces it and the games
appear normally. The message is the legacy `list_games` path surfacing an error
while the managed snapshot is still loading or before Steam's root is
discoverable; it is not an actionable failure for a managed library.

## Goal

Startup paints the managed loading state and then the library without ever
showing a transient Steam-installation error when a managed snapshot is
available or still loading.

## Non-goals

- Changing Steam root discovery rules.
- Hiding genuine backend failures when no managed library exists.

## Requirements

- R1: When a managed library is available, legacy Steam scan errors must not
  replace the managed shell with a red error message.
- R2: Genuine failures still surface in Settings → Doctor and when no managed
  library exists.
- R3: Startup must not flash the legacy skeleton before the managed shell is
  known.

## Acceptance criteria

- [ ] Launching SteamTron with a managed library shows the managed loading state
  and then the games; no `Steam installation was not found` text appears.
- [ ] Launching without any library still reports a real backend failure.

## Verification

Pending implementation.

## Decisions and risks

Queue item: handle after the focus regression in `0002-nav-focus.md`.

## Links

- Reported from a fresh-launch screenshot, 2026-11-15.
