# Per-game cache sidecar

- Status: active
- Date: 2026-11-16
- Areas: `crates/steamtron-library/src/{cache,backup,updates,lib}.rs`,
  `crates/steamtron-desktop/src/{main,dlc,download}.rs`

## Problem

DLC catalogue/size caches lived only in `~/.cache/steamtron`; the Updates cache
lived only in JavaScript memory. Per-game backups and global exports therefore
did not preserve them.

## Design

A sidecar next to each appmanifest:

```text
appmanifest_<appid>.cache.json
```

Schema:

```json
{
  "schema": 1,
  "game_id": "...",
  "steam_app_id": 123,
  "dlc": { "fetched_at": 0, "entries": [] },
  "updates": { "fetched_at": 0, "data": {} }
}
```

The appmanifest remains the source of truth for game configuration. Volatile
cache data lives only in the sidecar.

## Requirements

- R1: DLC reads/writes catalogue and sizes from the sidecar.
- R2: Updates reads/writes its result from the sidecar; explicit Refresh
  bypasses it.
- R3: Per-game backup copies the sidecar next to the appmanifest.
- R4: Restore and metadata import restore the sidecar.
- R5: Global export includes the sidecar; import reads it.
- R6: Download completion clears the Updates sidecar and session cache.

## Verification

- `cargo check` passes.
- Desktop tests: 20 passed.
- Library tests: 28 passed.
- Full restore/export round-trip with a populated sidecar was not run yet.

## Risks

- Restored sidecar data may be older than the current game state; explicit
  Refresh re-scans Updates.
- Sidecar is optional; old backups/exports remain valid.
