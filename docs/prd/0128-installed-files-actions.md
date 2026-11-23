# Installed Files actions follow the install location

- Status: active
- Date: 2026-11-23
- Areas: `steamtron-core` library load, `steamtron-desktop` restore and UI

## Problem

Rise of the Tomb Raider was restored into
`D:\SteamTronLibrary\steamtronapps\common\Rise of the Tomb Raider`, but its
library entry kept `installation.mode = "linked"` because the game already
existed as a linked entry before restore. Installed Files therefore showed
**Unlink** (with Uninstall disabled), and `uninstall_managed_game_at` refused
with "Unlink external games instead of deleting their folder."

Both actions are also always rendered, one of them disabled, which makes the
stored mode look authoritative even when the path says otherwise.

## Goal

Unlink is offered only for external linked games. Uninstall is offered for any
game whose content is in SteamTron's managed tree: restored from backup,
downloaded from the store, or linked and later moved onto a registered drive.

## Requirements

- R1: On library load, derive the mode from the content root:
  `<drive>/SteamTronLibrary/steamtronapps/common/…` → managed; anything else →
  linked. An empty root keeps the stored mode.
- R2: Restore writes the entry as `managed`.
- R3: Moving a linked game to a registered drive keeps writing `managed`.
- R4: Installed Files renders only the applicable action: Unlink for linked
  games, Uninstall for managed games. No disabled counterpart.
- R5: The backend guards stay: unlink refuses managed games, uninstall refuses
  linked external folders.

## Acceptance criteria

- [x] Core test: a linked mode with a managed content root normalizes to
      managed; a managed mode with an external root normalizes to linked.
- [ ] Rise of the Tomb Raider shows Uninstall only after installing 0.1.47.
- [ ] An external linked game still shows Unlink only.
- [ ] Uninstall works for the restored game and leaves the library entry.

## Verification

- Code: `cargo test -p steamtron-core derives_managed_mode` passes.
- Code: `cargo test -p steamtron-library` — 36 passed.
- Code: `cargo check -p steamtron-desktop --bin steamtron-desktop` passes.
- Code: `node --check crates/steamtron-desktop/ui/app.js` passes.
- Not run yet: installed-build action check.

## Decisions and risks

- Derive from the path, not from a game-specific rule or a one-time migration;
  every future restore/move gets the correct mode and existing entries are
  fixed on the next load.
- The mode is normalized in memory on load. It is persisted the next time a
  command saves the library; no background write is performed during reads.
- A game with an empty root keeps its stored mode (library-only entries are not
  reclassified).
