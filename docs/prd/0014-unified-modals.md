# Unified dialog windows

- Status: active
- Date: 2026-11-15
- Areas: `crates/steamtron-desktop/ui/app.js`, `crates/steamtron-desktop/ui/style.css`, `crates/steamtron-desktop/ui/index.html`

## Problem

The application uses thirteen separately built `.modal-backdrop` dialogs,
three static overlays, and twenty-four native `window.confirm` prompts. Headers
vary, only some dialogs have a close button, none has minimize or maximize, and
native confirms ignore the app's visual language. The settings surface is a
full-screen page with a decorative search glyph on the left and no window
controls.

## Goal

Every in-app dialog shares one window shell: title, minimize, maximize, and
close on the right; windowed by default; minimize parks the dialog in an
in-app dock and preserves its state; maximize expands it. Confirms are compact
custom windows. The settings surface uses the same window controls and has no
decorative search icon.

## Non-goals

- Replacing OS file/folder pickers (`rfd`); they stay native.
- Turning the sign-in gate into a dismissible window; it remains a blocking
  screen.
- Adding drag-to-move or resize handles.
- Changing any dialog's business logic or actions.

## Requirements

- R1: Every dynamic modal and the account dialog get one 3-button control group
  at the right of its header: minimize, maximize, close.
- R2: Default presentation is windowed. Maximize fills the usable window;
  minimize hides the dialog without destroying its DOM or state.
- R3: Minimized dialogs appear in an in-app dock; clicking an entry restores
  the same dialog and relaunches its focus.
- R4: All 24 `window.confirm` prompts are replaced by one custom confirm
  dialog. Its default width is **360px**, visibly smaller than other dialogs.
- R5: The settings surface becomes a windowed panel with the same controls;
  its decorative search glyph is removed. Close resets it to windowed mode.
- R6: The visible topmost dialog owns focus; Escape closes the topmost dialog,
  including nested dialogs, not the first in DOM order.
- R7: Dialog actions, close behaviors, and Escape-dismiss semantics remain
  unchanged, including dialogs that refresh the underlying route on close.

## Acceptance criteria

- [x] A custom dialog shows minimize, maximize, and close controls, in that
  order, on the right.
- [x] Minimize hides the dialog, adds a dock entry, and the dock restores the
  same dialog.
- [x] Maximize expands the dialog to the usable window area.
- [x] A confirm is 360px wide and uses the same window shell.
- [x] Settings opens windowed, can minimize/maximize, and has no search glyph.
- [x] All `window.confirm` call sites are gone.
- [x] Existing dialog flows, Escape handling, and navigation suites still pass.

## Verification

- `node --check crates/steamtron-desktop/ui/app.js` passed.
- jsdom suite `/tmp/steamtron-nav-test/panel-test.mjs` passed all 67 checks,
  including dialog controls, confirm window shell, dock restore, maximize,
  settings window controls, and Escape.
- Base navigation suite `test.mjs` passed 33/33; auth suite `auth-test.mjs`
  passed 13/13.
- `grep -n "window.confirm" crates/steamtron-desktop/ui/app.js` returns no
  matches.
- Release build completed and the installed binary hash matched the build.

## Decisions and risks

- The minimization target is an in-app dialog dock rather than separate OS
  windows; this keeps the single-window console layout and preserves dialog
  state.
- Existing dialog bodies and handlers are kept; the manager adds the shared
  shell and stack behavior, lowering migration risk.
- Nested dialogs are tracked in open order, so the newest dialog owns focus and
  Escape until it is closed or minimized.
- OS file/folder pickers remain native because they are the platform file
  browser, not app confirmation surfaces.

## Links

- Audit: prior dialog inventory in this session.
- Related: `docs/prd/0002-nav-focus.md`, `docs/prd/0013-guest-login.md`
