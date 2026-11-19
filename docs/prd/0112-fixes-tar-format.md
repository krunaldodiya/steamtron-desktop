# Fixes TAR format and backup updates

- Status: active
- Date: 2026-11-19
- Areas: `crates/steamtron-core/src/fixes.rs`,
  `crates/steamtron-library/src/managed.rs`,
  `crates/steamtron-desktop/src/main.rs`, `crates/steamtron-desktop/ui/app.js`

## Problem

Fixes export/import only handled ZIP. A SteamTron backup stores fixes as
`fixes.tar` (`fixes/<game folder>/...`), so updating a backup after fixing a
game required re-exporting and manually replacing the tar plus its SHA-256.
Large lua.tools packs also arrive as ZIP, so ZIP support cannot be removed.

## Goal

Export fixes in the exact backup layout, import either format, and update only
`fixes.tar` in an existing backup without touching `content.tar`.

## Requirements

- R1: `core::fixes::export_tar` writes `fixes/<folder>/<relative>` uncompressed
  TAR, atomic rename, and returns the SHA-256.
- R2: `install_tar_into` validates entries (regular files only, no `..`,
  absolute paths, links), strips one `fixes/<folder>/` prefix, and swaps the
  target atomically.
- R3: `install_archive_into` detects ZIP (`PK\x03\x04`) or TAR (`ustar`) and
  keeps ZIP import working.
- R4: Managed install uses the detected archive; Fixes UI imports ZIP/TAR and
  exports TAR.
- R5: `update_backup_fixes` rewrites only a backup's `fixes.tar` using the
  existing tar prefix (or `fixes_directory`), updates
  `components.fixes.sha256`, and leaves content/manifest archives untouched.

## Acceptance criteria

- [x] TAR round-trip preserves nested files and excludes the format marker.
- [x] Backups with an old prefix (e.g. `Assassins Creed Origins`) keep that
  prefix when updated.
- [x] ZIP import still works.
- [ ] UI export/import/update exercised on a rebuilt app. Not built yet per the
  on-request build policy.

## Verification

- `cargo test -p steamtron-core fixes::tests::tar_round_trips_backup_layout`,
  `archive_detection_rejects_unknown_files` and `tar_export_fits_a_small_stack`
  pass.
- `cargo check -p steamtron-desktop` passes.
- Runtime export on 0.1.34 initially crashed with `STATUS_STACK_OVERFLOW`; fixed
  by moving the 1 MiB hash buffer off the stack. UI re-test pending the rebuild.

## Decisions and risks

- Uncompressed TAR is required for backup compatibility; it is larger than ZIP.
- 64 GiB archive/extracted limits apply to both formats.
- Old ZIP export remains available through the legacy config path.

## Links

- Related: `0111-large-fixes-import`, `0110-unlink-keeps-library-entry`.
