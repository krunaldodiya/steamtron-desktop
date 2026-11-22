# Restore legacy and incomplete backups

- Status: active
- Date: 2026-11-22
- Areas: `steamtron-library` backup normalization, `steamtron-desktop` restore

## Problem

Restore only accepted appmanifests with `"backup_format": 2`. Older backups
carry `"format": 1` and no `backup_format`; their content layout is otherwise
the same (`content.tar` with `common/<folder>`, optional fixes and
`manifest-bundle.tar`). Mafia: Definitive Edition is one of these and fails
with `Unsupported SteamTron backup format.` on 0.1.45.

Some format-2 manifests also omit `components` or `content_directory` even
though the archives are present and compatible.

Separately, a physically damaged archive returns a raw
`Data error (cyclic redundancy check). (os error 23)` with no component name.
Max Payne 3 has this: its fixes and manifest archives match their checksums,
while `content.tar` is the damaged component. Code cannot recover missing bytes;
the error must say which archive failed and that the drive/archive is damaged.

## Goal

One restore path for every supported backup shape, and actionable errors for
damaged archives. No per-game handling.

## Requirements

- R1: Accept `backup_format: 2`, legacy `format: 1` (or no marker), and
  incomplete v2 manifests when `content.tar` exists.
- R2: Derive `content_archive`, `components.content`,
  `components.manifest_bundle`, and `components.fixes` from the files actually
  present. Use `content.tar` when the manifest has no archive name.
- R3: If `content_directory` is missing, read the first `common/<folder>` entry
  from `content.tar`.
- R4: For legacy fixes, find the archive by component name, `fixes.tar`, or
  `<title>.tar`; fixes remain optional.
- R5: Reject unknown explicit `backup_format` values and missing content with
  the existing unsupported-format error.
- R6: Write the normalized manifest into the restored library appmanifest; the
  backup folder itself is never modified.
- R7: A cyclic redundancy check (os error 23) on any component names that
  component and states that the archive or drive is damaged and must be
  re-exported. No claim of recovery.

## Acceptance criteria

- [x] Unit tests normalize a legacy format-1 manifest, infer the content
      directory, add the manifest bundle and legacy fixes, and reject an
      unknown format.
- [ ] Mafia: Definitive Edition restores on the installed build.
- [ ] Max Payne 3 reports `content.tar` as damaged instead of a raw OS error.

## Verification

- Code: `cargo test -p steamtron-library` — 36 passed, 0 failed.
- Code: `cargo check -p steamtron-desktop --bin steamtron-desktop` passes.
- Data: Max Payne 3 `Max Payne 3.tar` and `manifest-bundle.tar` match their
  appmanifest SHA-256 values; `content.tar` fails to read with
  `Input/output error` (CRC), confirming the damaged component.
- Not run yet: installed-build restores.

## Decisions and risks

- Normalize in memory, not by rewriting the backup; the restored library
  appmanifest is written from the normalized value.
- Legacy archives without a `common/` prefix are not inferred; they fail the
  existing content check rather than guessing a layout.
- A CRC means the data is gone; the only remedy is a fresh export or download
  to healthy storage.

## Links

- [0121](0121-portable-backup-name-safety.md) — backup format v2 and the G:
  drive's bad blocks.
- [0125](0125-restore-appmanifest-not-cache.md) — appmanifest selection.
