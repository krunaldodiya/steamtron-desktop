# Orphaned backup staging after a killed backup

- Status: draft
- Date: 2026-11-19
- Areas: `crates/steamtron-desktop/src/main.rs` (game backup task),
  `crates/steamtron-library/src/backup.rs`

## Problem

A game backup writes into `<destination>/.steamtron-backup-<pid>/<title>` and
only renames it to `<destination>/<title>` at the end. The error path removes
the staging folder, but a process kill never reaches that code.

On 2026-11-19, the RDR2 backup to `E:\Games\SteamTronGames` was killed when the
app process (pid 24744) was closed. It left:

```
.steamtron-backup-24744/Red Dead Redemption 2/content.tar
  9,183,056,896 bytes written of 128,327,779,626 total (~7.2%)
  last write 18:09:41
.steamtron-backup-24744/Red Dead Redemption 2/.manifest-cache/ (9 files)
```

`E:\Games\SteamTronGames\Red Dead Redemption 2` did not exist, so the backup
never completed. A later backup run uses `.steamtron-backup-<new pid>` and only
removes a staging folder with the same current pid, so the 9.19 GB partial is
never reused or cleaned and stays until someone deletes it by hand (it was
removed manually afterwards).

Because the backup is not resumable, the user also pays the full archive time
again for every killed run.

## Goal

A backup that is interrupted (crash, kill, app exit) never leaves an
indefinite orphan, and the next backup does not silently ignore space already
written by a previous one.

## Non-goals

- Resuming an archive byte-for-byte across runs (nice to have, not required to
  fix the leak).
- Changing the backup layout (`content.tar`, fixes TAR, manifest bundle,
  appmanifest) or the restore format.
- Touching completed backups at the destination.

## Requirements

- R1: When a game backup starts, remove or adopt every
  `<destination>/.steamtron-backup-*` folder that belongs to that game and has
  no live owner (owner pid no longer running, or folder older than the app's
  current session).
- R2: Never delete staging that the current process is using, or another live
  SteamTron instance is using.
- R3: Prefer adoption over deletion when the staging folder's metadata matches
  the same game and destination; otherwise delete it before starting.
- R4: Record the reason for a failed or abandoned backup in the per-game
  diagnostic log so the UI can say "last attempt was interrupted" instead of
  leaving a stale progress entry.
- R5: Cancellation through the UI, app shutdown, and hard kill all reach a
  cleanup path where possible; the next start covers the rest.

## Acceptance criteria

- [ ] Killing SteamTron mid-backup leaves at most a staging folder that the next
      backup for that game removes or reuses at start.
- [ ] A completed backup restores normally; no completed folder is modified by
      the cleanup.
- [ ] Two SteamTron instances cannot delete each other's live staging.
- [ ] A unit test covers stale detection (dead pid) and live-pid protection.

## Verification

Pending. The 9.19 GB orphan from the 2026-11-19 RDR2 attempt was deleted
manually; no automated cleanup exists.

## Decisions and risks

- Deleting unknown `.steamtron-backup-*` folders at the destination is risky if
  the user keeps unrelated files there; scope by the known game folder name and
  a metadata marker written at staging start.
- Reusing a partial `content.tar` requires validating its TAR end markers and
  the hashes recorded by a previous run; adoption can start as delete-only if
  that is too much for the first change.

## Links

- `0114-game-named-fixes-backup.md`, `0115-fixes-transfer-progress.md`.
- RDR2 game id `1174180`; backup destination `E:\Games\SteamTronGames`.
