# Appmanifest backups and per-game manifest folders

- Status: done
- Date: 2026-11-16
- Areas: `crates/steamtron-library/src/{backup,managed,lib,updates}.rs`,
  `crates/steamtron-desktop/src/main.rs`, `crates/steamtron-core/src/paths.rs`

## Problem

1. Restoring a per-game backup left no `appmanifest_<appid>.json` in the
   managed library. The backup contained the appmanifest metadata, but restore
   only rewrote `library.json` and the content tree.
2. Per-game exports wrote `metadata.json`, Lua, and depot manifests, but no
   appmanifest JSON.
3. The global manifest store `~/.local/share/steamtron/manifests` kept every
   depot manifest as a flat file, so files from different games mixed in one
   directory.

## Requirements

- R1: Game settings > Backup includes `appmanifest_<appid>.json`.
- R2: Restoring that backup writes the appmanifest JSON to
  `<steamapps>/appmanifest_<appid>.json`.
- R3: Global settings > Generate > Export includes
  `<title>/appmanifest_<appid>.json` in every game ZIP.
- R4: Global depot manifests live under
  `~/.local/share/steamtron/manifests/<appid>/<depot>_<manifest>.manifest`.
- R5: Add, regenerate, export, restore, verify, update checks, and remove use
  the per-game folders.
- R6: Existing flat files are migrated into per-game folders once at startup;
  flat Lua cache files move into their appid folder.

## Verification

- Exported 21 games; `Cricket 26.zip` contains
  `Cricket 26/appmanifest_3468650.json` with content, fixes, compatdata,
  launcher, and launch target fields.
- Ran the startup migration: 21 manifest folders, 0 flat files remaining.
- `cargo test -p steamtron-desktop --bin steamtron-desktop` passes.

## Decisions and risks

- The per-game backup's `appmanifest_<appid>.json` doubles as the backup
  descriptor and the restored appmanifest. It keeps `backup_format` and
  component hashes in addition to the appmanifest fields.
- Manifests remain in the global store after removing a game, matching the
  persistent bundle policy.
