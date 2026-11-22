# Restore selects the appmanifest, not its cache sidecar

- Status: active
- Date: 2026-11-22
- Areas: `steamtron-desktop` restore flow, `steamtron-library` backup import

## Problem

A format-2 backup folder contains both:

- `appmanifest_<appid>.json` — the real backup manifest, with
  `"backup_format": 2`,
- `appmanifest_<appid>.cache.json` — the DLC/update cache sidecar.

`restore_game_backup` looked for the first entry whose name starts with
`appmanifest_` and ends with `.json`. Both files match. NTFS lists
`appmanifest_<appid>.cache.json` before `appmanifest_<appid>.json`, so restore
read the sidecar, found no `backup_format`, and returned
`Unsupported SteamTron backup format.` at 0%.

Reproduced with
`G:\Games\SteamTronGames\The Witcher 3_ Wild Hunt` on 0.1.43. The real
appmanifest has `backup_format: 2` and the backup is complete. God of War worked
because its folder has no cache sidecar.

## Goal

Restore always selects the game appmanifest and ignores the cache sidecar.

## Non-goals

- Changing the backup format or the sidecar contents.
- Changes to metadata import (it already distinguishes cache from appmanifest).

## Requirements

- R1: The restore finder excludes `*.cache.json` and prefers the main
  `appmanifest_<appid>.json`.
- R2: A folder with only a cache file is not treated as a backup.
- R3: The sidecar is still copied after a successful restore when present.

## Acceptance criteria

- [x] Unit test: a folder containing both names returns the main appmanifest.
- [ ] The Witcher 3 backup restores on the installed build.
- [ ] Backups with no cache sidecar keep restoring as before.

## Verification

- Code: `cargo test -p steamtron-library` — 34 passed, 0 failed, including
  `backup::import_tests::backup_appmanifest_skips_the_cache_sidecar`.
- Code: `cargo check -p steamtron-desktop --bin steamtron-desktop` passes with
  the local GTK/WebKit pkg-config sysroot.
- Not run yet: installed-build restore of the Witcher 3 backup (a backup was
  running when this was written; do not interrupt it).

## Workaround on 0.1.43

After the current backup finishes, rename the sidecar in the backup folder:

```text
appmanifest_292030.cache.json  ->  appmanifest_292030.cache.json.off
```

Then Restore the folder. Rename it back afterwards if the DLC cache is wanted.
This is a data-file workaround, not a code change.

## Decisions and risks

- The finder is a shared helper (`steamtron_library::backup::backup_appmanifest`)
  so restore and future callers cannot accidentally match the sidecar again.
- If a future backup format drops the main appmanifest, restore will fail with
  the existing "missing appmanifest_*.json" message instead of reading the
  sidecar.

## Links

- [0121](0121-portable-backup-name-safety.md) — backup format v2 and restore
  validation.
- [0124](0124-restore-local-metadata.md) — restore without the manifest API.
