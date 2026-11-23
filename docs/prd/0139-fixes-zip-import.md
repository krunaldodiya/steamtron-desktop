# Fixes ZIP import and complete extraction

- Status: draft
- Date: 2026-11-23
- Areas: `steamtron-desktop` Fixes pane, `steamtron-library` fixes import

## Problem

The Fixes pane only accepts TAR files. The source fix packs on the user's drive
are ZIP files (for example `Grand Theft Auto V Enhanced.zip`). When that ZIP was
imported, the fixes folder contained only the five RUNE files; the ZIP's
`steam_api64.dll` and `steam_settings` were absent. This made GTA V launch fail
and loop because the GoldBerg part of the fix was missing.

The ZIP contains 13 files, including `steam_api64.dll`, `steam_settings/*`, and
`steamtron-fixes.json`; the fixes folder has only 5 of them.

## Goal

Import both TAR and ZIP fix packs, extract every file with the same path-safety
rules, and report files/sizes. A subsequent export must round-trip the complete
pack.

## Requirements

- R1: Accept `.tar` and `.zip` in the fixes picker and drag/drop path.
- R2: Reject absolute paths, `..`, drive-prefixed and symlink entries.
- R3: Preserve `steam_settings/` and other nested directories.
- R4: Use the same operation/progress/cancel UI as TAR import.
- R5: Do not overwrite an existing fixes folder without the existing import
  rules/confirmation.
- R6: Export remains complete and includes module-owned files.
- R7: No installed game file is modified.

## Acceptance criteria

- [ ] Importing `Grand Theft Auto V Enhanced.zip` produces all 13 files,
      including `steam_api64.dll` and `steam_settings/steam_appid.txt`.
- [x] Importing a malicious ZIP with absolute/`..` entries fails safely.
- [ ] Existing TAR import/export/remove tests still pass.
- [ ] The Fixes status reports the complete file count and size.

## Verification

- 0.1.62 added ZIP dispatch and `install_zip_into_with_progress`.
- Unit test `zip_import_keeps_nested_files_and_rejects_unsafe_paths` passed:
  nested `steam_settings/steam_appid.txt` extracted, `../evil.txt` rejected.
- Not yet imported through the installed app; not yet run against the actual
  GTA V ZIP.
- Evidence: current GTA V fixes folder had 5 files; the source ZIP has 13.

## Decisions and risks

- `zip` is already a dependency; no new library is required.
- The ZIP may contain an executable flagged by Defender. The existing security
  exclusion flow runs before import; it must remain in front of extraction.
- Keep the existing difference between additive import and explicit overwrite.

## Links

- PRD 0136 (launcher-bypass registry)
- GTA V loop evidence in PRD 0136
