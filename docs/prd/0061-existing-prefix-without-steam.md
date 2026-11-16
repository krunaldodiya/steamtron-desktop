# Use an existing Steam-style prefix without Steam

- Status: done
- Date: 2026-11-16
- Areas: `crates/steamtron-core/src/runtime.rs`

## Problem

Little Nightmares had `launch.prefix_mode = "steam"` with an existing Wine
prefix, but no Steam installation. Play failed with:

> Steam installation is not configured

The prefix existed at
`~/.local/share/Steam/steamapps/compatdata/424840/pfx` and contained
`pfx/system.reg` and `pfx/drive_c`.

## Requirements

- R1: Allow a Steam-mode prefix when Steam is absent only if the prefix is an
  existing Wine prefix.
- R2: Continue rejecting Steam mode when the prefix does not exist.
- R3: Do not recreate a fake Steam root.

## Verification

- `recognizes_an_existing_prefix_without_steam` passes.
- Runtime tests: 9 passed.
- Little Nightmares prefix contains `pfx/system.reg` and `pfx/drive_c`.

## Decisions and risks

- Prefix validation checks `system.reg` or `drive_c`, including one nested
  `pfx/` level.
- The stale target path is handled separately by PRD 0060.
