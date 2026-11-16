# Cache the Updates tab

- Status: done
- Date: 2026-11-16
- Areas: `crates/steamtron-desktop/ui/app.js`

## Problem

The Updates tab scanned local manifest stores every time it opened. It is not
network-backed like DLC, but repeated scans of large games were unnecessary.

## Requirements

- R1: Cache `update_versions` results per App ID for the session.
- R2: Open a cached tab without another backend scan.
- R3: Refresh explicitly re-scans.
- R4: Pin/unpin updates the cached data.
- R5: Download completion clears the update cache.

## Verification

- The tab renders cached data immediately and only calls `update_versions` on a
  cache miss or explicit Refresh.
- Pin and clear-pin write the returned data back to the cache.
- Download completion clears the cache.
- `node --check` passes.
