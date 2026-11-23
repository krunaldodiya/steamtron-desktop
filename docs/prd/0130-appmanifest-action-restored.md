# Restore the appmanifest action in Installed Files

- Status: active
- Date: 2026-11-23
- Areas: `steamtron-desktop` installed-files UI and command

## Problem

Game Settings → Installed Files used to show two local metadata actions:
**Generate app manifest** and **Generate Lua/manifests**. They were split in
`940cb26` and the appmanifest action was removed in `b12fcda` as "unused",
leaving only the Lua/manifest action.

The appmanifest is the local source of launch targets, launch options, paths and
library metadata. Users need to regenerate it after changing a launch option or
launcher without touching Steam or the provider bundle.

## Goal

Both actions are available side by side:

- **Generate app manifest** — rewrite the local appmanifest from the current
  library entry.
- **Generate Lua/manifests** — existing persistent-bundle restore.

## Requirements

- R1: Installed Files renders both buttons in one card.
- R2: The appmanifest action uses
  `steamtron_library::backup::game_appmanifest` as the single source of the
  manifest fields; no duplicated JSON construction.
- R3: Write atomically to `installation.manifest`, or to
  `<library>/appmanifest_<appid>.json` when the path is empty.
- R4: Preserve fields the builder does not own (for example `details`,
  `integrity`, `quicklook`) by merging into the existing JSON.
- R5: No Steam or network call. The Lua/manifest action is unchanged.
- R6: The button reports the written path and refreshes the tab.

## Acceptance criteria

- [x] Both buttons are present in Installed Files.
- [x] The appmanifest action writes the current launch options and targets.
- [x] Extra metadata fields survive a regeneration.
- [x] The Lua/manifest action still works independently.
- [ ] Installed-build check of both buttons.

## Verification

- Code: `cargo check -p steamtron-desktop --bin steamtron-desktop` passes.
- Code: `node --check crates/steamtron-desktop/ui/app.js` passes.
- Code: `generate_managed_appmanifest` is registered in the Tauri handler.
- Not run yet: installed-build button check.

## Decisions and risks

- Merge instead of overwrite: the backup builder intentionally omits
  `details`/`integrity`/`quicklook`, and a full overwrite would lose them.
- Keep the action game-local: it must work with no Steam install and no network.
- The removed implementation's duplicated JSON was not restored; the shared
  builder prevents the action from drifting from the backup format.

## Links

- `940cb26` — split appmanifest and Lua/manifest actions.
- `b12fcda` — removed the appmanifest action.
- [0036](0036-separate-appmanifest-lua-actions.md) — original action split.
