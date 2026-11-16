# DLC toggle without reloading the tab

- Status: done
- Date: 2026-11-16
- Areas: `crates/steamtron-desktop/ui/app.js`

## Problem

Checking or unchecking DLC called `renderDlcTab()` after the state change. That
replaced the whole DLC pane, re-fetched the Store DLC list and sizes, and caused
a visible reload. Steam updates only the row and the Play/Update action.

## Requirements

- R1: A toggle updates only the affected row and the primary action.
- R2: Do not re-fetch Store DLC metadata after a toggle.
- R3: Keep the checkbox state and re-enable it after success.
- R4: Revert the checkbox on failure.

## Verification

- The change handler no longer calls `renderDlcTab`.
- It updates the installed note, calls `updateManagedDlcAction`, and leaves the
  pane intact.
- `node --check crates/steamtron-desktop/ui/app.js` passes.
- No visual automation was available for the toggle itself.
