# Fullscreen Escape matches desktop back behavior

- Status: done
- Date: 2026-11-15
- Areas: `crates/steamtron-desktop/ui/app.js`

## Problem

In fullscreen mode Escape always opens the "Fullscreen options" dialog. In
desktop mode the same key closes details and dialogs and otherwise releases
focus. Because the fullscreen branch runs before the round-trip branches in
`dispatchNavigation`, a fullscreen Store or game detail cannot be left with
Escape, and a top-level Escape unexpectedly pops a system dialog.

## Goal

Escape means "back" in both display modes. Fullscreen options stay reachable
from the settings button and F11 still toggles the mode.

## Non-goals

- Changing how fullscreen mode is entered or left.
- Changing the fullscreen options actions (reboot, shutdown, quit, scale).
- Changing the F11 shortcut.

## Requirements

- R1: Escape in fullscreen closes an open modal or dialog first, like desktop.
- R2: Escape in fullscreen leaves a Store or game detail for its origin route.
- R3: Escape on a fullscreen list route does not create the fullscreen options
  dialog; it clears focus like desktop.
- R4: The settings button still opens fullscreen options in fullscreen mode,
  and Escape closes that dialog.
- R5: F11 continues to toggle fullscreen.

## Acceptance criteria

- [x] Fullscreen Store detail + Escape returns to the Store with the card
  focused and no options dialog.
- [x] Fullscreen game detail + Escape returns to its origin route.
- [x] Fullscreen list + Escape creates no visible modal.
- [x] Settings button in fullscreen opens Fullscreen options; Escape closes it.
- [x] Desktop-mode Escape behavior is unchanged.

## Verification

- `node --check crates/steamtron-desktop/ui/app.js` passed.
- jsdom harness `/tmp/steamtron-nav-test/panel-test.mjs` passed all 51 checks
  with `STORE_DELAY=3000 STORE_GAME_DELAYS="11:900,12:1800"` and a stateful
  fullscreen stub, including:
  - `Fullscreen Escape leaves a Store detail without opening options`
  - `Fullscreen Escape on a list opens no options dialog`
  - `Fullscreen Escape leaves a game detail without opening options`
  - `Settings opens Fullscreen options while fullscreen`
  - `Escape closes the Fullscreen options dialog`
- The base `/tmp/steamtron-nav-test/test.mjs` suite passed all 33 checks.
- Release build completed and the installed binary hash matched the build.

## Decisions and risks

- The fullscreen options dialog is already a `.modal-backdrop`, so it is
  closed by the generic modal branch before route handling. Removing the
  earlier auto-open branch is sufficient and keeps one Escape path.
- Fullscreen options remain reachable from the settings button; no new shortcut
  is added.

## Links

- Implementation: `crates/steamtron-desktop/ui/app.js`
- Related: `docs/prd/0002-nav-focus.md`
