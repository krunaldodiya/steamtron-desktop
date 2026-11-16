# Updates tab reads the persistent bundle

- Status: done
- Date: 2026-11-16
- Areas: `crates/steamtron-library/src/updates.rs`

## Problem

Every managed game showed:

> update error: This game has no managed stplug-in manifest.

The Updates tab still required a local `stplug-in` Lua file. Managed Lua files
were removed when the persistent bundle store became the source of truth.

## Requirements

- R1: Read a managed game's Lua from the persistent bundle when no local or
  Steam Lua file exists.
- R2: Preserve existing local/Steam Lua precedence.
- R3: Apply to every managed game, not just Cricket 26.

## Verification

Ran `available_updates` for all 21 library games:

- 21 OK, 0 errors.
- Version counts ranged from 1 to 36.
- Cricket 26: 1 version.
- The Binding of Isaac: 2 versions.
- Little Nightmares: 3 versions.

## Decisions and risks

- Update history still comes from the per-game manifest store and installed
  manifests; only Lua lookup changed.
