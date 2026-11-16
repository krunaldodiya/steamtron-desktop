# Per-game library metadata export

- Status: done
- Date: 2026-11-16
- Areas: `crates/steamtron-library/src/backup.rs`,
  `crates/steamtron-desktop/src/main.rs`,
  `crates/steamtron-desktop/ui/app.js`

## Problem

The library backup produced one ZIP containing `library.json` and a shared `lua/`
folder. In the current library only 5–6 of 20+ games had Lua files, so most
games could not be restored from the export. Users need one artifact per game,
with complete metadata, Lua and depot manifests.

## Goal

Export one ZIP per game:

```
<Game Title>.zip
  <Game Title>/metadata.json
  <Game Title>/lua/<appid>.lua
  <Game Title>/manifests/<depot>_<manifest>.manifest
```

Missing Lua/manifests are generated from the signed-in manifest API before the
ZIP is written. The export window reports `Exporting N of Total · Title` for each
game.

## Non-goals

- Exporting game content, fixes, prefixes, saves or artwork.
- Replacing the legacy library-ZIP restore path.
- Restoring the new per-game ZIPs in this change.

## Requirements

- R1: Choose a destination folder; write one `.zip` per library game.
- R2: Include `metadata.json` with the full `Game` record.
- R3: Include `lua/<appid>.lua`; generate it when missing.
- R4: Parse `setManifestid` from Lua and include every manifest, generating
  missing manifests through the API bundle and caching them locally.
- R5: Report progress per game with the current index/total and title.
- R6: Support Cancel between games through the operation control.
- R7: Overwrite an existing per-game ZIP only after the new ZIP is complete
  (temporary file + rename).

## Acceptance criteria

- [ ] A 20-game library produces 20 ZIPs.
- [ ] Every ZIP contains metadata, Lua and manifests where available.
- [ ] Missing Lua/manifests are generated and counted.
- [ ] The window shows `Exporting 1 of 20`, `2 of 20`, etc.
- [ ] Cancel stops before the next game.

## Verification

- Ran `export_game_backups` against the real 21-game library into a temporary
  folder.
- Result: 21 ZIPs, 293 files, 16 missing Lua files generated, 0 manifests still
  missing.
- Verified every ZIP contains `lua/<appid>.lua` and manifests.
- Sample `Little Nightmares.zip`:
  - `Little Nightmares/metadata.json`
  - `Little Nightmares/lua/424840.lua`
  - three depot manifests under `Little Nightmares/manifests/`.
- Removed the temporary exporter and test output afterwards.

## Decisions and risks

- API generation can take time for many missing games; progress names the
  current game while the ZIP is prepared.
- A missing manifest that the provider cannot supply is counted in
  `missing_manifests` rather than silently omitted.
- Legacy restore still accepts the old single library ZIP.

## Links

- Related PRD: `docs/prd/0019-consolidated-backup.md`
