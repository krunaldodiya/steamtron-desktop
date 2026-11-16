# Keep the Downloading button while DLC runs

- Status: done
- Date: 2026-11-16
- Areas: `crates/steamtron-desktop/ui/app.js`

## Problem

After clicking Update, the detail action behind the download manager still said
Update. `updateManagedDlcAction` re-ran while DLC was downloading, saw selected
DLC as not yet installed, and overwrote the Downloading action with Update.
Closing the dialog triggered a re-render that restored the correct label.

## Requirements

- R1: Do not apply the pending-DLC Update label while a download is active.
- R2: Resume normal Play/Update handling after the download reaches a terminal
  state.

## Verification

- `updateManagedDlcAction` returns early for a nonterminal
  `managedDownloads` entry.
- `node --check` passes.
