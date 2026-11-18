# Restored game keeps its artwork

- Status: done
- Date: 2026-11-18
- Areas: `crates/steamtron-desktop/ui/app.js`

## Problem

A game restored from a backup (Cricket 26, app 3468650) showed "No cover art"
on Home and Library although `%LOCALAPPDATA%\steamtron\artwork\3468650_cover.jpg`
and `_hero.jpg` existed (written during the restore at 19:13).

`artworkFor(gameId)` cached the result of `managed_artwork` even when it was
`null`. The cards were painted before the restore wrote the files, and every
later render reused the cached `null`.

## Goal

Artwork that appears during a restore is visible without restarting SteamTron.

## Requirements

- R1: Do not cache a failed or empty `managed_artwork` result.
- R2: Clear the artwork cache when a backup restore completes so already
  painted cards fetch again.

## Acceptance criteria

- [x] A failed `managed_artwork` lookup is not cached; the next lookup sees files
  written by a restore.
- [x] The restored game's card shows its cover without a restart.

## Verification

- Installed build `003770e126810351726a4bdbd30177cfabf1d120336ee07c3b59afffcc1c77eb`
  at 19:57 matches `target/release`. CDP against the running app:
  `artworkFor('3468650')` returned
  `C:\Users\krunaldodiya\AppData\Local\steamtron\artwork\3468650_cover.jpg`
  and `_hero.jpg`; a lookup for a missing game left `artworkCache.has(...)`
  false; the Cricket 26 card rendered an image instead of "No cover art".
- `3468650_cover.jpg` (221,636 bytes) and `3468650_hero.jpg` (1,611,526 bytes)
  exist on disk, so the missing image was UI caching, not missing artwork.

## Decisions and risks

- `artworkFor` runs per render; retrying a genuinely missing artwork adds one
  IPC call per paint. Failed lookups are still a single call, not a loop.
- The cache is process-wide; clearing it on restore costs one artwork fetch per
  visible card.

## Links

- Related: `0099-store-settings-loading`.
