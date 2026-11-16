# DLC download manager state

- Status: done
- Date: 2026-11-16
- Areas: `crates/steamtron-desktop/{src/dlc.rs,src/download.rs,ui/app.js}`

## Problem

The DLC download manager showed:

> The download is no longer running.

even while DLC progress events continued and the game later showed Play. The
manager polls `managed_download_status`, but DLC states were only emitted, never
stored in the download state map. A second race made the first poll occur
before the DLC task stored its initial state.

## Requirements

- R1: Store DLC download state in the same map as base game downloads.
- R2: Keep the manager on the local state while the backend task starts.
- R3: Do not show an error for a nonterminal local state.

## Verification

- `dlc::start` now uses `publish_and_store` for every state update.
- The manager falls back to its local nonterminal state when
  `managed_download_status` returns `None`.
- `node --check` passes.
- Desktop tests pass.
