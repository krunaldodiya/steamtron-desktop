# Restore backups from their own metadata

- Status: active
- Date: 2026-11-22
- Areas: `steamtron-desktop` restore flow, `steamtron-library` backup import

## Problem

Restoring a game backup when the game is not in `library.json` calls
`add_managed_game` with `ManifestProvider::Api` just to recreate the library
entry. That call downloads `{api_base}/api/download/{appid}?file_type=manifest`
with a 180-second timeout.

A complete format-2 backup already contains everything needed for the entry:

- `appmanifest_<appid>.json` (game id, title, app id, launch targets, settings),
- `manifest-bundle.tar` with `<appid>.lua`, `<appid>.smd.lua`, and the depot
  manifests,
- optional `appmanifest_<appid>.cache.json`.

Restore then replaces the downloaded appmanifest with the backup copy anyway.
The download is therefore pure overhead and a new failure point.

Observed on 2026-11-22: restoring *God of War* and *Grand Theft Auto V
Enhanced* after they were removed from the library failed at 0% after
3 m 29 s with `manifest download failed: error decoding response body`. The
backups were readable and complete; only the API call failed.

## Goal

A complete backup restores without any network access. The library entry is
built from the backup's own appmanifest and manifest bundle, exactly as the
existing "Import folder…" metadata action does.

## Non-goals

- Changing the Add game / update flows, which legitimately download a manifest
  bundle from the providers.
- Registering a restored game with Steam. Restore only rebuilds the entry.
- Repairing malformed backups or archives.

## Requirements

- R1: When restore finds no library entry, build it from the backup folder via
  the existing metadata-import path (`read_folder` + `import_one`), not the API.
- R2: No manifest API request is made while restoring a format-2 backup.
- R3: The new entry uses a registered drive and the normal
  `<drive>/steamtronapps/common/<title>` layout. Content, fixes, manifest, and
  appmanifest restoring is unchanged.
- R4: Existing library entries still skip entry creation and restore over them.
- R5: A backup without `appmanifest_*.json` fails with a clear message.
- R6: A fixes archive is restored whenever `components.fixes` is present, even
  if `fixes_directory` is missing from the appmanifest. The backup's
  `fixes_enabled` value is applied before the commit, and the fixes archive is
  committed after the content (content, then fixes, then manifests).
- R7: Fixes are optional. A backup without a fixes component (or with a stale
  `fixes_directory` and no archive) restores without requiring fixes and leaves
  any existing fixes folder untouched.

## Acceptance criteria

- [x] Unit test imports a backup folder with an appmanifest and manifest
      bundle, returns the game, and creates no content and no network traffic.
- [ ] Restoring God of War with the network blocked completes without an API
      request.
- [ ] A restored entry has the same App ID, title, root, and launch target as
      the metadata import of the same folder.
- [ ] A backup whose appmanifest has `components.fixes` but no
      `fixes_directory` still restores the fixes archive.
- [ ] A backup without fixes restores successfully and keeps the game's existing
      fixes folder.

## Verification

- Code: `cargo test -p steamtron-library` — 34 passed, 0 failed, including
  `backup::import_tests::imports_a_backup_entry_without_content_or_network`.
- Code: fixes replacement only runs when a fixes archive was staged, and a
  stale `fixes_directory` without an archive is ignored; a backup without fixes
  leaves the existing fixes folder untouched.
- Code: the God of War fixes archive starts with `fixes/God of War/…`, matching
  the restore's `TarComponentLayout::Bundle(content_folder)` mapping; the commit
  runs content first, then fixes, then manifests, and sets `fixes_enabled` from
  the backup.
- Code: `cargo check -p steamtron-desktop --bin steamtron-desktop` passes with
  a locally extracted GTK/WebKit pkg-config development sysroot (no system
  packages installed); only pre-existing warnings remain.
- Not run: an installed-build restore. Until the next build, only the library
  tests and code inspection are evidence.

## Decisions and risks

- Reuse the metadata import instead of duplicating the `Game` construction in
  the restore command. `import_one` now returns the stored `Game` so restore
  can continue with its layout; metadata import ignores the return value.
- The old path registered the game with a ready Steam backend as a side effect
  of `add_managed_game`. The local import does not. Registration remains
  available through the existing registration/repair flow, and is not needed on
  Windows; this is recorded as a known behavior change.
- A backup with a missing or invalid appmanifest now fails at entry creation.
  Restore already rejected backups without `appmanifest_*.json`, so the visible
  behavior is unchanged apart from the error text.
- Fixes replacement is conditional on a fixes archive actually being staged. A
  backup without fixes leaves any existing fixes folder for that game intact;
  the old code moved it aside even when there was nothing to replace it with.

## Links

- [0121](0121-portable-backup-name-safety.md) — backup/restore naming and
  component repair.
- [0114](0114-game-named-fixes-backup.md) — game-named fixes archive.
