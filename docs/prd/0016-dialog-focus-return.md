# Return focus to the dialog opener

- Status: done
- Date: 2026-11-15
- Areas: `crates/steamtron-desktop/ui/app.js`

## Problem

Opening Settings or the Account dialog from the top-right controls with the
keyboard and closing it with Escape left focus on the document body. The same
happened for other dialogs whose close handler repainted the underlying route.
Keyboard users lost their place and had to restart navigation from the header.

## Goal

Closing a dialog returns focus to the control that opened it. Settings returns
to its launcher button; Account returns to the profile button; nested dialogs
return to their opener inside the parent dialog.

## Non-goals

- Persisting opener focus across application restarts.
- Changing dialog stacking, Escape order, or backdrop behavior.

## Requirements

- R1: Settings captures its opener and restores focus to it on Escape or close.
- R2: Every registered dialog captures the active element as its opener and
  restores focus when it closes.
- R3: If the opener was rebuilt by a route repaint, restore by its `id` (for
  example `#managed-options`).
- R4: A nested dialog restores focus inside the still-open parent dialog, not
  the page behind it.
- R5: The sign-in gate focuses its Sign in action when it appears.

## Acceptance criteria

- [x] Settings opened from the gear returns focus to the gear on Escape.
- [x] Account opened from the profile button returns focus to it on Escape.
- [x] Game Settings returns focus to its options opener after its close repaint.
- [x] Nested dialogs keep focus inside the parent dialog.
- [x] Existing dialog, navigation, and auth suites pass.

## Verification

- jsdom dialog suite `/tmp/steamtron-nav-test/panel-test.mjs`: 73/73 checks
  pass, including `Escape returns focus to the settings launcher` and
  `Closing Game Settings restores focus to its opener`.
- Base navigation suite `test.mjs`: 34/34, including
  `Escape returns focus to the profile button`.
- Auth suite `auth-test.mjs`: 13/13.
- `node --check crates/steamtron-desktop/ui/app.js` passed.
- Release build completed and the installed binary hash matched the build.

## Decisions and risks

- Openers are captured from `document.activeElement` at registration. If the
  opener was removed by a repaint, the same `id` is looked up in the new DOM.
- Focus restoration is skipped while the auth gate is visible; the gate takes
  focus to its Sign in button instead.
- If another dialog remains open, focus is restored only when the opener lives
  inside that parent dialog.

## Links

- Implementation: `crates/steamtron-desktop/ui/app.js`
- Related: `docs/prd/0014-unified-modals.md`
