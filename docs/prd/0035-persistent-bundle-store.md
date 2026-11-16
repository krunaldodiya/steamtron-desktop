# Persistent global Lua/manifest bundle store

- Status: done
- Date: 2026-11-16
- Areas: `crates/steamtron-library/src/lib.rs`,
  `crates/steamtron-desktop/src/main.rs`

## Problem

Manifest ZIPs were cached under the disposable cache directory
(`~/.cache/steamtron/manifests/api`). Clearing the cache, or moving between
library add/remove flows, could trigger the signed-in API again for the same App
ID even though the complete Lua + manifest ZIP was already downloaded.

## Goal

Keep one persistent bundle per App ID under SteamTron's data directory and use it
for add, remove/re-add, export, backup and verification without another API call.
Existing disposable-cache bundles migrate automatically.

## Requirements

- R1: Store bundles at `<SteamTron data>/manifest-bundles/<appid>.zip`.
- R2: `download_bundle` reads the persistent store before the old cache/API.
- R3: Existing `cache/manifests/api/*.zip` bundles move to the persistent store.
- R4: New API downloads write both the persistent store and the legacy cache.
- R5: Removing a game never deletes the persistent bundle.
- R6: Startup runs the migration asynchronously.

## Acceptance criteria

- [x] Existing bundles migrate to the data directory.
- [x] Add and export reuse a stored bundle without an API request.
- [x] A missing bundle still falls back to the API and is persisted.

## Verification

- Ran `migrate_manifest_cache` against the real cache.
- Result: 21 ZIPs moved to
  `/home/krunaldodiya/.local/share/steamtron/manifest-bundles`.
- The per-game export test also populated this store for missing games.

## Decisions and risks

- Bundles are intentionally retained across library removal.
- No TTL is applied; a stale bundle can be refreshed only when it is missing or
  malformed. A future explicit refresh action can replace it.
- The disposable cache is still written for compatibility with older code paths.

## Links

- Related PRD: `docs/prd/0034-per-game-library-export.md`
