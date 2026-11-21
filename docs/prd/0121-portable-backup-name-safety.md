# Portable backup archives restore on Windows and Linux

- Status: active
- Date: 2026-11-21
- Areas: `crates/steamtron-desktop/src/main.rs`,
  `crates/steamtron-library/src/backup.rs`, `crates/steamtron-core/src/fixes.rs`
  (call sites only)

## Problem

Restoring a whole-game backup fails at 100% with a bare OS error after the
entire content archive has already been copied into the staging folder.

Evidence from `G:\Games\SteamTronGames\Marvel's Spider-Man_ Miles Morales`
(backup written 2026-11-12, metadata rewritten 2026-11-16):

- `manifest-bundle.tar` stores its wrapper folder as
  `manifests/Marvel's Spider-Man: Miles Morales` while the same backup's
  `content_directory` and `fixes_directory` use the Windows-safe
  `Marvel's Spider-Man Miles Morales`.
- On Windows, `CreateDirectoryW` on the `:` name returns
  `ERROR_DIRECTORY` (267), which is the exact message the UI showed:
  `The directory name is invalid. (os error 267)`.
- The restore had already extracted 47.4 GiB of `content.tar` (its SHA-256
  matched the manifest) into
  `D:\SteamTronLibrary\steamtronapps\.restore-15464`; the installed game folder
  was never created or modified.
- 8 of the 31 backup folders under `G:\Games\SteamTronGames` carry the same
  invalid wrapper (`A Plague Tale: Innocence`, `A Plague Tale: Requiem`,
  `Hellblade: Senua's Sacrifice`, `Mafia: Definitive Edition`,
  `Marvel's Spider-Man: Miles Morales`, `Sekiro: Shadows Die Twice`,
  `Sleeping Dogs: Definitive Edition`, `Uncharted: Legacy of Thieves
  Collection`).
- The affected archive was also unreadable in one 64 KiB region on disk
  (`robocopy`, `CopyFile2`, and `certutil` reported CRC / `STATUS_IN_PAGE_ERROR`
  while small reads and the tar headers worked), so fixing the name alone was
  not enough for that folder.

Two writer gaps produced this class of backup:

- The metadata-repair path in `steamtron-library/src/backup.rs` passed the raw
  game title to `write_manifest_tar`, so a title with `:` became an invalid
  wrapper folder. (Current per-game backup writes already use the installed
  folder name.)
- The legacy fixes export command passed the raw game title as the fixes TAR
  wrapper, so the same defect can appear in fixes archives.

The whole-game backup already writes the fixes component in the backup layout
(`<game>.tar` recorded in `components.fixes.name`, entries under
`fixes/<installed folder>/...`) and the Installed Files tab calls the same
`backup_managed_game` command, but the file name comes from a local
`safe_folder` copy instead of the standalone fixes export's `export_name`, so
the two can disagree for long or unusual titles.

## Goal

A backup created on Linux or Windows restores on either platform when the game
itself is portable, and an archive name that the current platform cannot create
fails in seconds with an actionable message instead of after a multi-gigabyte
copy. Whole-game backups keep writing the fixes component exactly like the
standalone fixes export.

## Non-goals

- Verifying `content.tar` checksums before extraction; that reads the entire
  archive (0046 deliberately removed the pre-scan) and restore already verifies
  content after extraction.
- Repairing damaged disks or recovering unreadable archive bytes automatically;
  that stays a manual/data-repair task.
- Changing the backup format version or the recorded component names.
- Rewriting existing backup folders as part of the app; the eight affected
  folders are repaired once by hand and documented here.

## Requirements

- R1: Restore ignores the wrapper folder of the fixes and manifest-bundle TARs
  and places every entry under the installed folder recorded in the backup's
  `content_directory` (one folder below the component root).
- R2: Restore rejects archives whose entry names cannot be created on the
  current platform (Windows: `< > : " | ? *`, control characters, trailing
  space/dot, reserved device names; any platform: NUL, empty, absolute, `..`)
  with a message naming the entry.
- R3: Restore validates the fixes and manifest-bundle TARs before extracting
  `content.tar`, so a backup that cannot be restored fails before the large
  copy; `content.tar` is not pre-scanned.
- R4: The game folder from `content_directory` is checked before the restore
  task starts, so a Linux backup whose folder name Windows cannot create fails
  immediately.
- R5: Entries directly at a component root keep their name under the installed
  folder, and a wrapper directory entry creates the installed folder; nothing
  is written outside the staging folder.
- R6: `steamtron-library/src/backup.rs::write_manifest_tar` sanitizes its
  wrapper folder and `repair_backup_manifests_with_zips` prefers the
  appmanifest `content_directory` folder, so repaired bundles never regress.
- R7: The whole-game backup names its fixes component with
  `steamtron_core::fixes::export_name`, matching the standalone "Export fixes
  TAR" action, and keeps the `fixes/<installed folder>/...` layout recorded in
  `components.fixes.name`.
- R8: The legacy config-based fixes export uses the fixes folder's own name for
  the TAR wrapper instead of the raw title.
- R9: Behaviour is identical on Linux and Windows; the mapping is not
  Windows-only, because the same wrapper mismatch also makes restore silently
  skip manifests and fixes on Linux.

## Acceptance criteria

- [ ] A manifest bundle whose wrapper is `manifests/<title with ':'>` restores
      its files under `manifests/<content_directory folder>` on Windows and
      Linux.
- [ ] Restore of a backup whose fixes or manifest-bundle TAR has a name the
      platform cannot create fails before `content.tar` is extracted and names
      the offending entry.
- [ ] A normal backup (wrapper already equal to the installed folder) restores
      with unchanged file contents.
- [ ] `content.tar`, fixes, manifests, appmanifest, and the library entry are
      committed exactly as before on success.
- [ ] A whole-game backup's `components.fixes.name` equals
      `fixes::export_name(title)` and its TAR entries start with
      `fixes/<installed folder>/`.
- [ ] `cargo test -p steamtron-desktop` covers the mapping, the unsafe paths,
      and the Windows-only invalid name (where the host runs that platform).

## Verification

- Data: all 31 folders under `G:\Games\SteamTronGames` were audited by reading
  every fixes and manifest-bundle member. 20 are format 2 and now pass (the
  TAR wrapper equals the `content_directory` folder and the declared SHA-256
  matches the file); 13 of them were rewritten:
  - manifest and fixes wrappers rebuilt to the installed folder
    (`Marvel's Spider-Man: Miles Morales`, `Sekiro: Shadows Die Twice`,
    `Far Cry 5` fixes and manifest, `Assassin's Creed Origins` fixes);
  - five stale component SHA-256 values refreshed;
  - five missing `manifest_bundle` components added (their `manifest-bundle.tar`
    existed but restore skipped it);
  - two stale `fixes_directory` values cleared because the folder has no fixes
    archive (Hitman, Lies of P);
  - one stale fixes component name repointed (Assassin's Creed Origins);
  - 14 legacy `fixes.tar` files renamed to `<game>.tar`, matching the
    standalone fixes export and the current writer.
  The 14 format-1 folders (no `backup_format`) are still rejected as
  unsupported by this build.
- The Miles bundle was unreadable in one 64 KiB region
  (`ERROR_CRC`/`STATUS_IN_PAGE_ERROR`). Windows logged Event ID 7,
  "The device, \Device\Harddisk2\DR2, has a bad block", at the same times;
  Harddisk2 is the USB Samsung M3 Portable that holds G:. The rebuilt archive
  re-reads all 18 entries and its 15 files that also exist in the cached
  1817190 bundle match it byte for byte. The backup folders keep only their
  archives and appmanifest; the pre-repair copies live outside the backup tree
  in `%TEMP%\opencode\backup-repair-2026-11-21`.
- Code: `cargo test -p steamtron-library` passes (32 tests), including the
  backup/import suite. `cargo test -p steamtron-core` passes 54 of 55; the
  failure is `runtime::tests::terminates_a_process_group` (`kill -TERM` cannot
  signal the group inside WSL) and is unrelated.
- Code: `cargo check -p steamtron-desktop` passes (Linux, with a locally
  extracted pkg-config sysroot); the seven warnings are pre-existing.
- Code: the mapping helpers were extracted verbatim from `main.rs` and built
  with `rustc --test`: five tests pass, including the legacy
  `manifests/Marvel's Spider-Man: Miles Morales/...` wrapper, nested fixes
  files, the stray nested wrapper directory, and unsafe-path rejection.
- Code: `rustfmt --check` parses both changed files; the repository has
  pre-existing formatting differences with the current rustfmt, so a global
  `cargo fmt` was not applied.
- Not run: a full restore with the rebuilt, installed app. The repaired Miles
  backup is also restorable by 0.1.39 because its wrapper is now valid.
- Release 0.1.40 (workflow run 44919200367, Linux and Windows builds green)
  published
  https://github.com/krunaldodiya/steamtron-desktop/releases/tag/v0.1.40 with
  `SteamTron_0.1.40_x64-setup.exe` (SHA-256
  `c09363d4a1c19c32b34a81455528cf2af0a535fbc407f69dbb4fa9d6a6b1e515`, matching
  the published `SHA256SUMS.txt`), `SteamTron_0.1.40_amd64.deb`,
  `SteamTron-0.1.40-1.x86_64.rpm` and `SteamTron_0.1.40_amd64.AppImage`. The
  installer was downloaded to `C:\Users\krunaldodiya\Downloads` and was not
  installed in this session.

## Decisions and risks

- The wrapper folder is treated as archive decoration, not data: the installed
  folder comes from the trusted appmanifest `content_directory`. This keeps
  legacy backups (raw titles, sanitized titles, app-id folders) restorable
  without guessing a sanitizer that must match the writer.
- Game content and fixes payloads keep their archive paths (only the wrapper is
  replaced), because renaming payload files would break the game. Names that
  the host cannot create are rejected with a clear message instead of being
  mangled.
- Early validation reads the small components only (a few MiB). Validating
  `content.tar` would reintroduce the multi-minute pre-scan rejected in 0046.
- Risk: a backup whose payload contains a Windows-invalid name still fails when
  restored on Windows; the message now explains why and which entry.
- Risk: two game folders that sanitize differently but map to the same managed
  folder could collide; that already exists in the layout rules and is not
  changed here.
- The G: backup drive has bad blocks (Windows Event ID 7 against
  `\Device\Harddisk2`), so one metadata archive was partly unreadable. Restore
  does not verify the declared SHA-256 of the fixes and manifest-bundle
  archives (only `content.tar` is checked after extraction). Adding strict
  verification is deliberately not part of this change: the same repair bug
  left stale component hashes in existing backups, and rejecting on mismatch
  would turn a stale hash into a failed restore. The new pre-flight validation
  still reads every header and surfaces media errors before the content copy.
- Existing format-1 backups are not migrated; upgrading them would mean
  writing a new format version into user data without a content check.
- Repair policy: only metadata wrappers, component names, and hashes were
  rewritten; no payload file name was renamed and no archive content was
  replaced except the one unreadable manifest recovered from the cached
  bundle.

## Links

- `0046-backup-progress-streaming.md` (no full pre-scan).
- `0112-fixes-tar-format.md`, `0114-game-named-fixes-backup.md`,
  `0116-linked-fixes-managed-location.md`.
- `0119-orphaned-backup-staging.md` (leftover staging after a killed/failed
  restore).
