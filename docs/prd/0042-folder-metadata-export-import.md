# Folder metadata export and import

- Status: done
- Date: 2026-11-16
- Areas: `crates/steamtron-library/src/{backup,lib,managed}.rs`,
  `crates/steamtron-desktop/{src/main.rs,ui/app.js}`

## Problem

Global export wrote ZIPs containing both `metadata.json` and a generated
appmanifest, which did not match the per-game backup folder or the exact
managed appmanifest. Removing the legacy restore left no way to import the
exported library metadata back.

## Requirements

- R1: Export one folder per game using the per-game backup layout, omitting only
  `content.tar` and `fixes.tar`.
- R2: The folder contains `appmanifest_<appid>.json` copied exactly from
  `game.installation.manifest`.
- R3: The folder contains `manifest-bundle.tar` with Lua, SMD Lua, and depot
  manifests.
- R4: No `metadata.json` and no ZIP wrapper are produced.
- R5: Import accepts one or more backup ZIPs or a folder containing ZIPs.
- R6: Import also accepts exported folders containing `appmanifest_*.json`.
- R7: Import writes the exact appmanifest, restores the persistent bundle, and
  adds or updates the library entry.

## Verification

- Exported 21 games to `/tmp/steamtron-export-folder`; each is a folder with
  `appmanifest_<appid>.json` and `manifest-bundle.tar`.
- Isaac's exported appmanifest is byte-identical to
  `/mnt/krunal/SteamTronLibrary/steamtronapps/appmanifest_113200.json`.
- Imported that export folder into a temporary library: 21 imported, 0 skipped,
  and Isaac's appmanifest remained byte-identical.
- Tests `imports_a_game_folder_and_writes_the_exact_appmanifest` and
  `imports_an_exported_zip` pass.

## Decisions and risks

- Imported metadata without game content creates an unconfirmed library entry;
  full content still comes from the per-game format-2 backup.
- Legacy consolidated ZIPs (`steamtron-backup.json`) are skipped rather than
  interpreted.
