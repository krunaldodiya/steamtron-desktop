# Export/import settings UX

- Status: done
- Date: 2026-11-16
- Areas: `crates/steamtron-desktop/{ui/app.js,src/main.rs}`

## Problem

- The maintenance button stayed on “Exporting game backups…” after export.
- The idle label was “Export games…” instead of a simple “Export”.
- Import buttons did not reliably show a native picker.

## Requirements

- R1: Reset the export operation button after success or failure.
- R2: Label the idle button “Export”.
- R3: Show a native multi-file picker for Import ZIPs.
- R4: Show a native folder picker for Import folder.
- R5: Separate path selection from import work so the picker runs as a
  synchronous Tauri command on the main thread.
- R6: Import runs in the same operation-manager dialog as export, with
  per-game progress such as “Importing 1 of 21” and a 0–100% bar.

## Verification

- `finishOperation` and `failOperation` now always reset the operation button.
- `pick_import_paths` opens `pick_files` or `pick_folder`; import receives the
  selected paths in a second command.
- Ran the debug binary with a temporary picker hook: both `files` and `folder`
  modes blocked with the native dialog open until the 6-second timeout, proving
  the picker opens inside Tauri.
- Import progress emits 22 callbacks for 21 ZIPs: `0/21 "Importing 1 of 21"` …
  `21/21 "Import complete"`.
- The import button no longer uses `busy()`; progress is shown by the operation
  manager modal.
- `node --check crates/steamtron-desktop/ui/app.js` passes.
- Desktop tests pass.

## Decisions and risks

- The picker and the import operation are separate commands; cancelling the
  picker performs no library work.
