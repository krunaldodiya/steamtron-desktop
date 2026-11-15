# Keep Home focus after sign-in window refocus

- Status: done
- Date: 2026-11-15
- Areas: `crates/steamtron-desktop/ui/app.js`

## Problem

After signing in through the browser and returning to SteamTron, the Home
screen rendered with no focused card. The window `focus` listener refreshs
running state and repaints the route; the repaint replaced the focused card
with new DOM, and no focus was restored. A second async focus event could also
restore focus after the user had already moved it.

## Goal

Returning focus to the app keeps a card focused when the previous focus was in
the route body, and never steals focus from the header or another surface.

## Non-goals

- Changing the running-state poll.
- Changing route focus memory or rail behavior.

## Requirements

- R1: A window-focus repaint restores the route's remembered card when the
  pre-repaint focus was inside the route body.
- R2: A window-focus repaint does not steal focus from the header, a modal, or
  settings.
- R3: A stale async focus handler does not restore focus after the user moved
  it elsewhere.

## Acceptance criteria

- [x] Home card focus survives a window-focus repaint.
- [x] Header focus survives a window-focus repaint.
- [x] Existing route focus, dialog, and navigation checks still pass.

## Verification

- jsdom `/tmp/steamtron-nav-test/panel-test.mjs`: 71/71 checks pass, including
  `Window refocus keeps a Home card focused` and
  `Window refocus keeps header focus`.
- Base suite 33/33; auth suite 13/13.
- Release build completed and the installed binary hash matched the build.

## Decisions and risks

- The listener captures the focused element before awaiting the running-state
  refresh and restores only when that element is still the active element and
  focus was inside the route body or lost.
- A disconnected remembered card no longer forces a restore when the user has
  valid focus elsewhere.

## Links

- Related: `docs/prd/0005-route-focus.md`, `docs/prd/0013-guest-login.md`
