# Repair backup metadata without redoing content

- Status: done
- Date: 2026-11-16
- Areas: `crates/steamtron-library/src/backup.rs`

## Problem

20 of 31 existing backup folders had no `manifest-bundle.tar`, and 6 of the 11
bundles had no Lua. Re-running full backups just to restore metadata would copy
tens of gigabytes again.

## Requirements

- R1: Repair `manifest-bundle.tar` without reading or rewriting `content.tar`
  or `fixes.tar`.
- R2: Prefer Lua/manifests from an existing exported ZIP.
- R3: Fall back to the persistent bundle, API, then SteamML.
- R4: Merge any existing depot manifests instead of dropping them.
- R5: Write the repaired tar atomically and skip already-complete bundles.

## Verification

- Actual repair on `/mnt/SAMSUNG-76F2FB0E7EBB795E/Games/SteamTronGames`:
  - scanned 31, repaired 16, already complete 15, skipped 0.
  - 31/31 bundles now contain Lua and at least one depot manifest.
- Newest `content.tar` timestamp remained `10:40`; repair wrote bundles at
  `10:52`, so content was not touched.
- Tests:
  - `repaired_manifest_tar_contains_lua_and_manifests`
  - `repairs_a_missing_bundle_from_an_export_zip`

## Decisions and risks

- The repair reads metadata only. `content.tar` and `fixes.tar` are never
  opened for writing.
- Old export ZIPs in `/home/krunaldodiya/Downloads/steamtron library backup`
  supplied most bundles; remaining games used the online providers.
