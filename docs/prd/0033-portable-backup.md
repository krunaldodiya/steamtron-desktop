# Portable backup folders (format 2)

- Status: active
- Date: 2026-11-15
- Areas: `crates/steamtron-desktop/src/main.rs`,
  `crates/steamtron-desktop/ui/app.js`

## Problem

The current Backup writes one `.tar.gz`, while the previous SteamTron/UmuTron
backup format is a portable **folder**:

```
The Binding of Isaac/
  appmanifest_113200.json
  content.tar
  fixes.tar
  manifest-bundle.tar
```

Existing backups in that format cannot be restored by the current app. The
reference backup at
`/mnt/SAMSUNG-76F2FB0E7EBB795E/Games/SteamTronGames/The Binding of Isaac`
contains `backup_format: 2`, component SHA-256 values, `content.tar` under
`common/<game>/`, `fixes.tar` under `fixes/<game>/`, and manifest files under
`manifests/<game>/`.

## Goal

Backup creates the same portable folder layout, and Restore accepts it by
choosing the folder. Metadata, fixes, manifests, launch targets, Proton mode,
and fixes state are preserved.

## Requirements

- R1: Backup destination is a chosen parent folder; output is
  `<parent>/<Game Title>/`. If it exists, the first command returns an `exists`
  result without starting; the UI asks Overwrite/Keep old backup and retries with
  the same parent only on Overwrite.
- R2: Write `content.tar`, `fixes.tar` (when present), and
  `manifest-bundle.tar` (when manifests are available).
- R3: Write `appmanifest_<appid>.json` with format 2, directories, launch
  settings/targets, proton, fixes state, and SHA-256 component metadata.
- R4: Archive content as `common/<folder>/...`, fixes as `fixes/<folder>/...`,
  and manifests as `manifests/<folder>/...`.
- R5: Restore chooses a backup folder, validates `backup_format: 2`, then
  extracts to staging while hashing content.tar in the same read. It shows one
  continuous 0–100% byte progress, verifies the checksum before replacing the
  game root, and has no separate checksum phase.
- R6: Restore moves fixes/manifests into place, rebuilds launch targets, marks
  the game installed, and saves it.
- R7: Cancel or failure during extraction discards the staging folder and leaves
  the installed game untouched. Commit moves the old content aside, installs the
  new content, and rolls back on commit/cancel failure; staging is removed after
  the library save.
- R8: Existing `.tar.gz` backups are no longer the primary format.

## Acceptance criteria

- [ ] A new backup has the reference folder structure.
- [ ] The reference backup restores into the library.
- [ ] Content, fixes, manifests, launch targets, and plugin settings restore.
- [ ] Checksum mismatch is rejected.
- [ ] Cancel leaves the existing game untouched.

## Verification

- Inspected the reference backup: `appmanifest_113200.json` has
  `backup_format: 2`, `content_directory: common/The Binding of Isaac`, content
  and fixes SHA-256 values; `content.tar` contains `common/The Binding of
  Isaac/...`; `fixes.tar` contains `fixes/The Binding of Isaac/...`;
  `manifest-bundle.tar` contains depot manifests.
- Removed the silent full-tar pre-scan; extraction progress uses archive file
  sizes as the denominator.
- Checksum is computed from the same stream used for extraction, so there is
  no separate verify pass, no second progress phase, and no artificial 10%
  ceiling.
- Extraction copies 1 MiB chunks and emits progress every 200 ms, so the bar
  moves smoothly even when one file is many gigabytes.
- Restore of the 9.77 GB Little Nightmares backup completed: its staging folder
  grew incrementally, the content root became 9.2 GB, and the library entry
  became confirmed with an Atlas/Binaries/Win64 launch target. The UI was
  corrected so checksum verification no longer reports 100% before extraction.
- Fixed a SIGSEGV during checksum: the hashing function allocated a 4 MiB buffer
  on a tokio blocking-thread stack. The buffer is now heap-allocated.

## Decisions and risks

- Restoration extracts content, fixes, and manifests into `.restore-<pid>`
  staging first. The target is replaced only after checksum verification and a
  final cancel check. Commit keeps the old target/fixes in staging until the
  library save succeeds, so cancel/failure rolls back instead of leaving half
  files.
- The backup folder name is sanitized from the game title. An existing folder is
  never overwritten without an explicit in-app confirmation; the old backup is
  replaced only after the new package is fully staged.
- Legacy single archive restore is removed from the primary path.

## Links

- Reference: `/mnt/SAMSUNG-76F2FB0E7EBB795E/Games/SteamTronGames/The Binding of Isaac`
- Related PRDs: `docs/prd/0030-operation-manager.md`
