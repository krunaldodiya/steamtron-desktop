# Cache the DLC catalogue

- Status: done
- Date: 2026-11-16
- Author: agent
- Areas: `crates/steamtron-desktop/{src/main.rs,ui/app.js}`

## Problem

The DLC tab said “Loading DLC from Steam…” (it uses the Store API, not a Steam
client) and re-fetched the base game and every DLC entry on every tab open.
This made the tab take many seconds.

## Requirements

- R1: Cache DLC IDs, names, and release dates for 30 days.
- R2: Bypass all Store API requests on a valid cache hit.
- R3: Keep selected/installed state and manifest availability live from local
  data, not the cache.
- R4: Rename the loading text so it does not imply Steam is installed.

## Verification

- Cache is written to `~/.cache/steamtron/dlc-catalog.json`.
- `managed_dlc` reads the cache before calling the Store API.
- `node --check` passes.
- Desktop tests pass.

## Decisions and risks

- A newly released DLC can take up to 30 days to appear unless the cache file
  is removed.
- Sizes remain in the separate `dlc-sizes.json` cache.
