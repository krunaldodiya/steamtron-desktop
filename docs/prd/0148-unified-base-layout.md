# Unified `.steamtron-base` layout for all managed games

- Status: done
- Date: 2026-11-24
- Areas: `steamtron-desktop` library/play planning, Windows overlay setup

## Problem

The managed library is a mix of layouts: some games keep content directly in
`common/<Game>` and use a drive-letter overlay, while others use
`.steamtron-base/<Game>` with a junction and a real-path sidecar. This makes
restore, backup, and troubleshooting inconsistent, and forces the user to
reason about which game uses which structure.

The Mount Manager real-path flow is now proven on GTA V Enhanced and Little
Nightmares II. The library should use one symmetrical layout for every managed
game.

## Goal

Every managed game has the same Windows layout:

- content in `common/.steamtron-base/<Game>`;
- `common/<Game>` is a junction to that base while idle;
- the real path is mounted through WinFsp for Play;
- a real-path sidecar exists so restore/backup and Play agree.

Linked/external games keep their existing read-only reference and are not
moved.

## Non-goals

- Moving linked games or game files outside the managed `common` folder.
- Changing the backup archive format.
- Adding per-game catalog preparation flags.

## Requirements

- R1: At startup, prepare the `.steamtron-base` layout for every managed game
  that does not have it yet. The operation is idempotent.
- R2: A managed game without a sidecar is prepared on Play even if the startup
  migration has not run yet.
- R3: Linked/shortcut games are skipped.
- R4: If preparing a game fails, Play falls back to the drive-letter overlay
  for that game instead of failing the library.
- R5: The base content is never written by Play; writes copy up into the
  overlay upper layer.
- R6: The real-path junction is restored to `.steamtron-base` after Stop.
- R7: Restore/backup continues to work with the base layout.

## Acceptance criteria

- [x] Every managed game has a base folder, a real-path junction and a
      sidecar after startup. 17/17 passed; a live game legitimately shows a
      mount instead of a junction while it is running.
- [x] GTA V Enhanced, Far Cry 5, Max Payne 3 and newly migrated games
      (Little Nightmares II, Watch Dogs 2) start through the real path and Stop
      restores the junction.
- [x] Base hashes stay unchanged across a Play/Stop run.
- [x] Linked games remain untouched. The code skips every mode other than
      `Managed`; the current library has no linked entries.
- [ ] A preparation failure does not prevent the game from launching through
      the drive-letter fallback. Not exercised because every managed game
      prepared successfully.

## Verification

- 0.1.74 verified the real-path layout manually on GTA V Enhanced and Little
  Nightmares II: `mount_mode=mount-manager`, base hashes unchanged,
  `AUTO_RESTORE=PASS`.
- 0.1.75/0.1.76 runtime: 17/17 managed games have base + junction + sidecar;
  `.steamtron-mount` no longer exists.
- GTA V Enhanced, Far Cry 5 and Max Payne 3 Play/Stop: real-path
  `mount_mode=mount-manager`, `AUTO_RESTORE=PASS`, base hashes unchanged.
- Little Nightmares II and Watch Dogs 2 were migrated from direct folders and
  launched through the new layout. No per-game preparation was used.

## Decisions and risks

- `ensure_real_path` is idempotent and renames within the same volume; it does
  not copy content.
- A crash between the rename and junction creation is recoverable because the
  base remains and `repair_game_root` recreates the junction.
- A game with both a direct root and an existing base is skipped and falls back
  rather than merging two content trees.
- Startup migration runs in the existing background maintenance thread, so it
  does not block the first UI frame.

## Links

- PRD 0135 (real-path WinFsp overlay)
- PRD 0147 (Mount Manager volume identity)
- PRD 0146 (non-elevated GUI)
