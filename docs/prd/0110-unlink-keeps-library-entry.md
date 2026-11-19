# Unlink keeps the library entry

- Status: active
- Date: 2026-11-19
- Areas: `crates/steamtron-desktop/src/main.rs`, `crates/steamtron-desktop/ui/app.js`

## Problem

Unlinking an external linked game (Assassin's Creed Origins) called
`remove_managed_game`, which deleted the library entry. The user lost the game
card, metadata, and artwork and had to add it again. Unlink should only remove
the link; removing the entry is a separate decision.

## Goal

Unlink clears the launchers, install confirmation, and Steam registration but
keeps the library entry, metadata, artwork, fixes, and files. **Remove from
library** remains the explicit delete action.

## Requirements

- R1: Add `unlink_managed_game`; it only accepts `InstallMode::Linked`.
- R2: It unregisters the Steam Lua entry, clears `launch_targets`,
  `installation.confirmed`, and `setup_completed_at`, and saves the game.
- R3: It returns the refreshed library so the UI updates without a reload.
- R4: `installed` requires `installation.confirmed`, and the extra settings tabs
  follow `installed` only, so an unlinked external game shows the basic tabs and
  a disabled Play until it is relinked.
- R5: The Files pane explains that unlink keeps the entry and files.
- R6: Managed-game Uninstall and Remove from library behavior is unchanged.

## Acceptance criteria

- [ ] Unlinking a linked game leaves its card in Home/Library.
- [ ] Its launchers and Steam registration are gone; the files stay on disk.
- [ ] Game Settings shows only General, Compatibility (Linux), Environment,
  Debug, and Game Link after unlink.
- [ ] Remove from library still deletes the entry.

## Verification

- Code inspection and `cargo check`/`node --check`; runtime check on the built
  0.1.32 app recorded below.

## Decisions and risks

- The game stays `InstallMode::Linked` with no targets so it can be relinked
  from the same folder.
- Artwork and fixes remain; app-data fixes are not deleted on unlink.

## Links

- Related: `0025-uninstall-keeps-library`, `0109-windows-parity-link-tray-fixes`.
