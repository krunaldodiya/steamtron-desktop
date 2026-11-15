# Library index backup and restore

- Status: done
- Date: 2026-11-15
- Areas: `crates/steamtron-core/src/library.rs`, `crates/steamtron-desktop/src/main.rs`, `crates/steamtron-desktop/ui/app.js`

## Problem

There is no way to export or restore the whole managed library document
(`library.json`: settings, drives and games). The only backup controls save
Steam-facing metadata (Lua files, appmanifests and fixes), not the library
index. After reinstalling, the drives and game entries must be recreated by
hand even when the user has a copy of the JSON file.

The user holds a prior export named `steamtron-library.json` that is byte-for-byte
the active `library.json`.

## Goal

Settings → Storage offers Export and Import for the library document. Export
writes an exact copy; Import validates the file, keeps the current document as
`library.json.bak`, replaces the index, and refreshes the UI.

## Non-goals

- Moving game content, Proton prefixes, saves, or artwork.
- Reconnecting drives whose mount points no longer exist.
- Automatic or scheduled backups.
- Changing the existing Steam metadata ZIP backup.

## Requirements

- R1: Storage shows a Library backup section with Export… and Import… actions.
- R2: Export copies the active `library.json` byte-for-byte to the chosen
  `steamtron-library.json` path.
- R3: Import reads a JSON document, requires the supported library version,
  and runs the same validation as normal library loading.
- R4: A failed or invalid import leaves the active library untouched.
- R5: A successful import copies the previous active document to
  `library.json.bak` before replacing it.
- R6: After import the UI reloads the library, drives, and settings without a
  restart.
- R7: Import warns that it replaces the current drives, games, and settings.

## Acceptance criteria

- [x] Storage shows Library backup with Export… and Import… buttons.
- [x] Exporting a known library produces an identical file.
- [x] Importing that file restores the same drives and games.
- [x] Invalid JSON or an unsupported version is rejected and the active file
  is unchanged.
- [x] The pre-import file remains at `library.json.bak`.
- [x] The UI reloads the imported library and reports the game and drive
  counts.

## Verification

- `cmp -s /mnt/krunal/WorkSpace/steamtron-library.json ~/.local/share/steamtron/library.json`
  passed; the user's saved export is byte-identical to the active version-2
  library document.
- `cargo test -p steamtron-core library::tests --lib` passed all 11 tests,
  including `exports_and_imports_the_library_document` (exact bytes, counts,
  and `library.json.bak`) and
  `invalid_import_leaves_the_active_library_untouched` (malformed JSON and
  unsupported version).
- `cargo check -p steamtron-desktop` passed.
- jsdom harness `/tmp/steamtron-nav-test/panel-test.mjs` passed all 54 checks,
  including Storage controls, `export_library_file`, and
  `import_library_file` invocation.
- Release build completed and the installed binary hash matched the build.

## Decisions and risks

- The imported bytes are written as-is so a backup round-trip is exact; this
  also preserves unknown fields for a future library version.
- Validation happens before any write. The `.bak` write happens before the
  replacement, so a failed copy cannot discard the current index.
- Paths inside the imported document are only checked for being absolute and
  safe. A drive that is not currently mounted remains listed; content is not
  moved or probed during import.

## Links

- Implementation: `crates/steamtron-core/src/library.rs`,
  `crates/steamtron-desktop/src/main.rs`,
  `crates/steamtron-desktop/ui/app.js`
- Related: `docs/prd/0006-store-cache.md`
