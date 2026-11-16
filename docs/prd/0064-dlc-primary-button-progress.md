# Primary button reflects DLC download progress

- Status: done
- Date: 2026-11-16
- Areas: `crates/steamtron-desktop/ui/app.js`

## Problem

The detail action behind the open manager kept saying Update. While an inline
progress element exists, `applyDownloadEvent` only updated the inline progress
and never changed the primary Play/Update button. It changed to Downloading
only after closing the dialog and forcing a full re-render.

## Requirements

- R1: Set the primary button to Downloading immediately when a DLC update starts.
- R2: Keep it as Downloading on each nonterminal download event, even when an
  inline progress element is present.
- R3: Let a terminal event re-render the action back to Play.

## Verification

- `startManagedDlcUpdate` sets the button before opening the manager.
- `applyDownloadEvent` updates the button when inline progress exists.
- Terminal events call `renderManagedMode`.
- `node --check` passes.
