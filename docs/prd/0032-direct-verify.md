# Verify with cached manifests through DepotDownloaderMod

- Status: active
- Date: 2026-11-15
- Areas: `crates/steamtron-desktop/src/main.rs`

## Problem

Verify for Cricket 26 (App 3468650) failed immediately at 0% with:

```
App 3468650 is not present in any configured manifest source.
Integrity verification failed. See the diagnostic log.
```

The API had the manifest bundle (`3468650.lua` and
`3468651_3365192551612268361.manifest`), and direct DDM verification with a
local `steam.keys` + manifest succeeded. SMD's `--update --repair` path probes
its configured sources first and rejects the app even when valid local manifest
data exists.

## Goal

Verify works from cached/api-provided depot metadata without requiring SMD's
manifest source probe. Missing manifests are fetched once and reused.

## Requirements

- R1: Parse depot IDs and manifest IDs from the game Lua.
- R2: Write `steam.keys` from keyed `addappid` entries.
- R3: Reuse manifests from the game root or SteamTron's manifest cache; fetch the
  API bundle only when a manifest is missing.
- R4: If DepotDownloaderMod is available, verify each depot directly with
  `-verify-all` and report real percent.
- R5: Pause/resume/cancel applies to the DDM process group.
- R6: Fall back to the existing SMD path when DDM is unavailable.

## Acceptance criteria

- [ ] Cricket 26 verify no longer fails at the source probe.
- [ ] Verify reads files and reports real progress.
- [ ] Pause/resume/cancel work during DDM verification.
- [ ] A second verify reuses manifest files without refetching.

## Verification

- API bundle for App 3468650 returned `3468650.lua` and a 27 MB manifest.
- Direct command
  `DepotDownloaderMod -app 3468650 -depot 3468651 -manifest ... -depotkeys steam.keys -manifestfile ... -dir . -verify-all`
  ran with anonymous Steam login and started validating files.
- `cargo check` passed.

## Decisions and risks

- DDM is copied from SMD's extracted temporary path into SteamTron's tools
  directory on first use. If SMD has never extracted it, verify falls back to the
  SMD path.
- Verification repairs missing/damaged files because DDM uses `-verify-all`,
  matching the existing verify promise.
- Fetching a missing manifest bundle can take several seconds; the manager shows
  "Preparing depot metadata…".

## Links

- Related PRDs: `docs/prd/0029-ddm-engine.md`,
  `docs/prd/0030-operation-manager.md`
