# Fix Doctor install action reference

- Status: done
- Date: 2026-11-18
- Areas: `crates/steamtron-desktop/ui/app.js`

## Problem

In Settings → Steam checks, activating **Install BetterSteamTools** failed with
`UI error: ReferenceError: backendName is not defined`.

`backendName` was declared inside `loadDoctor`, but the settings Doctor check
actions (`data-doctor-repair`) and the dialog repair button referenced it from
another scope. Any missing-backend repair therefore threw before opening its
confirmation dialog.

## Goal

Every Doctor repair button opens its confirmation and runs the backend action
without a reference error on Linux and Windows.

## Requirements

- R1: Provide one shared backend display name derived from `IS_WINDOWS`.
- R2: Replace all `backendName` references, including the local declaration in
  `loadDoctor`.
- R3: Do not change the repair commands or confirmations.

## Acceptance criteria

- [x] No `backendName` identifier remains in `app.js`.
- [x] Activating the BetterSteamTools repair shows the confirmation dialog and
  no `ReferenceError`.
- [x] Cancel leaves the system unchanged.

## Verification

- `node --check` passes.
- Installed `0.1.26` build: Doctor checks listed **Install BetterSteamTools**;
  clicking it opened the "Repair Steam components" confirmation with
  **Cancel/Repair** and no error dialog. A page scan for
  `backendName is not defined` was false.

## Decisions and risks

- The label is UI-only; the backend command names are unchanged.

## Links

- Related: `0090-minimal-doctor`.
