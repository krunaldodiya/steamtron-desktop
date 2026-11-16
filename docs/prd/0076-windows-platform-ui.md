# Hide Linux-only Proton UI on Windows

- Status: active
- Date: 2026-11-17
- Areas: `crates/steamtron-desktop/ui/app.js`

## Problem

The Windows build shows **Proton Manager**, the Proton compatibility tab, the
default prefix source and Feral GameMode. Those controls manage Linux Proton
runners and have no meaning for native Windows games.

## Requirements

- R1: Hide the Proton Manager settings tab on Windows.
- R2: Hide the Compatibility game-settings tab on Windows.
- R3: Hide default prefix source and GameMode rows on Windows.
- R4: Keep all Linux behavior unchanged.
- R5: Do not remove backend commands; only the irrelevant UI.

## Verification

- `node --check`.
- Windows VM: Settings shows General and Storage only; Game Settings has no
  Compatibility tab.
- Linux: all existing tabs and controls remain.
