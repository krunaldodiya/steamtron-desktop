# Remove the startup "Steam installation was not found" flash

- Status: done
- Date: 2026-11-15
- Areas: `crates/steamtron-desktop/ui/app.js`

## Problem

On a fresh launch the Library painted a red `Steam installation was not found`
message for a few seconds. The managed library then replaced it and the games
appeared normally. The message came from the legacy `list_games` Steam scan. In
`load()` any failure in that legacy block was treated as fatal, even when a
managed library snapshot was available.

## Goal

Startup paints the managed loading state and then the home screen without ever
showing a transient Steam-installation error when a managed snapshot exists.

## Non-goals

- Changing Steam root discovery rules.
- Hiding genuine backend failures when no managed library exists.

## Requirements

- R1: When a managed library is available, legacy Steam scan errors must not
  replace the managed shell with a red error message.
- R2: Genuine failures still surface when no managed library exists.
- R3: Startup must not flash the legacy skeleton before the managed shell is
  known.

## Acceptance criteria

- [x] Launching with a managed library and a failing `list_games` call still
  paints the managed Home screen and its cards.
- [x] Launching without a managed library still reports the real backend error.
- [x] The managed loading state is generic ("Loading your games…") rather than
  claiming the Library route before Home is painted.

## Verification

- jsdom harness with `list_games` forced to reject with
  `Steam installation was not found` passed every navigation check and rendered
  the Home cards, proving the legacy error no longer replaces the shell.
- `node --check crates/steamtron-desktop/ui/app.js` — no syntax errors.
- The built application was launched and inspected: no red startup error, and
  the Home route painted after loading.

## Decisions and risks

- `list_games` and its fixes/launch-target helpers are still attempted for the
  legacy shell. Failures are recorded and only rethrown when no managed
  snapshot exists, so real legacy-only failures remain visible.

## Links

- Related: `docs/prd/0002-nav-focus.md`, `docs/prd/0004-default-home.md`
