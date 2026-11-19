# Fixes TAR import and export

- Status: done
- Date: 2026-11-19
- Areas: `crates/steamtron-core/src/fixes.rs`,
  `crates/steamtron-library/src/managed.rs`,
  `crates/steamtron-desktop/src/main.rs`, `crates/steamtron-desktop/ui/app.js`

## Problem

Fixes export/import only handled ZIP. Users need one simple format that matches
the backup layout `fixes/<game folder>/...` for manual backup work.

## Goal

Export and import fixes as a backup-layout TAR.

## Requirements

- R1: `core::fixes::export_tar` writes `fixes/<folder>/<relative>` uncompressed
  TAR, atomic rename, and returns the SHA-256.
- R2: `install_tar_into` validates entries (regular files only, no `..`,
  absolute paths, links), strips one `fixes/<folder>/` prefix, and swaps the
  target atomically.
- R3: The Fixes UI and CLI import and export TAR only.

## Acceptance criteria

- [x] TAR round-trip preserves nested files and excludes the format marker.
- [x] UI export/import exercised on the rebuilt 0.1.34 app.

## Verification

- `cargo test -p steamtron-core fixes::tests::tar_round_trips_backup_layout`,
  `archive_detection_rejects_unknown_files` and `tar_export_fits_a_small_stack`
  pass.
- `cargo check -p steamtron-desktop` passes.
- Runtime export on 0.1.34 initially crashed with `STATUS_STACK_OVERFLOW`; fixed
  by moving the 1 MiB hash buffer off the stack. Rebuilt and re-tested.
- Installed build 0.1.35 (`%LOCALAPPDATA%\Programs\SteamTron`, hash
  `17af2cb0…`): Fixes shows only Import fixes TAR and Export fixes TAR (no ZIP
  or backup-update action). Exported 582160 to TAR, imported 424840 TAR
  (3 files), and rejected a ZIP fixture with `fixes archive is invalid`.
- Program Files replacement is still pending: the UAC prompt was denied, so the
  final binary was installed per-user with a desktop shortcut.

## Decisions and risks

- Uncompressed TAR matches the backup layout; no compression is applied.
- 64 GiB archive/extracted limits apply to both formats.

## Links

- Related: `0111-large-fixes-import`, `0110-unlink-keeps-library-entry`.
