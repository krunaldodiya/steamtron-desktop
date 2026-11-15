# Uninstall removes game files without removing the library entry

- Status: done
- Date: 2026-11-15
- Areas: `crates/steamtron-desktop/src/main.rs`,
  `crates/steamtron-desktop/ui/app.js`

## Problem

The Files tab's Uninstall action deleted the game folder **and** removed the
game from the library, then navigated away. That conflates two separate user
decisions: freeing disk space and removing the library entry. After an
uninstall the user should still see the game in Library/Home so they can
download it again, and remove the entry explicitly only if they want to.

## Goal

Uninstall deletes installed content but keeps the game in the library with its
metadata, artwork, Steam registration, fixes, prefix, and download destination.
The game page immediately shows Download and remains available until the user
chooses Remove from library.

## Non-goals

- Changing Remove from library, which keeps files on disk and removes the entry.
- Deleting saves, Proton prefixes, fixes, or artwork.
- Changing what happens to linked games; they remain Unlink-only.

## Requirements

- R1: Uninstall deletes the managed game content folder only.
- R2: The library entry stays after uninstall.
- R3: Metadata, artwork, Steam registration, fixes, prefix, and install path stay.
- R4: `installation.confirmed` is cleared, launch targets are cleared, and
  `setup_completed_at` is cleared so the game is not treated as playable.
- R5: An active download is cancelled before the content folder is deleted.
- R6: The confirmation dialog says files are deleted and the game stays in the
  library; one confirmation is enough.
- R7: After uninstall the game settings close and the detail page shows Download.
- R8: Remove from library remains a separate action on the game page.

## Acceptance criteria

- [ ] Uninstall deletes the game folder.
- [ ] The game remains in the library and on Home/Library lists.
- [ ] The detail page shows Download after uninstall, not Play.
- [ ] Re-downloading uses the same library entry and install path.
- [ ] Remove from library still removes the entry without deleting files.
- [ ] Linked games can still only be unlinked.

## Verification

- `cargo test -p steamtron-desktop --bin steamtron-desktop` passed 10/10,
  including `uninstall_tests::uninstall_removes_content_but_keeps_the_library_entry`.
  The test removes the content folder, then reopens the library and verifies the
  entry still exists with cleared confirmation, launch targets, and setup stamp.
- JSDOM uninstall harness passed: Uninstall invoked the backend, closed Game
  Settings, and the detail page showed Download instead of Play.
- Existing navigation suites still pass.
- `node --check crates/steamtron-desktop/ui/app.js` passed.

## Decisions and risks

- Steam registration is retained because the library entry and App ID are
  retained. Removing the entry still unregisters as before.
- Fixes and prefixes are retained; they are per-game configuration/save state,
  not installed content.
- The entry keeps `installation.root`, so a later Download targets the same
  folder instead of creating a second copy.

## Links

- Related PRD: `docs/prd/0024-download-manager.md`
- Implementation: `crates/steamtron-desktop/src/main.rs`,
  `crates/steamtron-desktop/ui/app.js`
