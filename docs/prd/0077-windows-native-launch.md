# Native Windows game launch

- Status: done
- Date: 2026-11-17
- Areas: `crates/steamtron-core/src/runtime.rs`, `crates/steamtron-desktop/src/main.rs`

## Problem

The Windows desktop build still planned managed launches through a Proton
runner. Proton and Wine prefixes are Linux-only; Windows games should start
their `.exe` directly.

## Requirements

- R1: Windows launch plans run the selected target executable directly.
- R2: Use the game's working directory, arguments and environment.
- R3: Do not require or inspect a Proton runner on Windows.
- R4: Keep Linux Proton planning unchanged.
- R5: Retain process tracking, logs, launch targets and stop behavior.

## Acceptance

- [ ] Windows managed game starts without Proton.
- [ ] Linux managed launch still uses Proton.
- [ ] Windows stop ends the game process.
- [ ] No Proton-related error appears on Windows Play.
