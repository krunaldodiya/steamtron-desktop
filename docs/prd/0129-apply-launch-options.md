# Apply game launch options to every launch path

- Status: active
- Date: 2026-11-23
- Areas: `steamtron-core` runtime plan, `steamtron-desktop` overlay command

## Problem

Game-level launch options are saved and written to the appmanifest as
`launch_options` (Rise of the Tomb Raider has `["-nolauncher"]`), but
`managed_launch_plan` built its arguments only from the selected target's
`arguments`. The overlay launch branch in `launch_game` also emitted only the
target arguments. The option therefore never reached the game process.

## Goal

Game-level launch options and target arguments are both passed to the game on
native Windows, Proton, and overlay launches.

## Requirements

- R1: Combine `game.launch.arguments` (game-level, `launch_options`) followed by
  `target.arguments`.
- R2: Use the combined list in both `managed_launch_plan` variants.
- R3: In the overlay path, pass the combined list exactly once; do not duplicate
  target arguments.
- R4: No change when either list is empty.
- R5: The existing appmanifest fields and UI save path stay unchanged.

## Acceptance criteria

- [x] Core test combines `-nolauncher` with the target's `-windowed`.
- [x] The Proton plan test still passes and includes the combined list.
- [ ] Rise of the Tomb Raider starts with `-nolauncher` on the installed build.
- [ ] A game with no launch options behaves as before.

## Verification

- Code: `cargo test -p steamtron-core combines_game_and_target_launch_arguments`
  passes.
- Code: `cargo test -p steamtron-core plans_proton_with_prefix_and_app_id`
  passes.
- Code: `cargo check -p steamtron-desktop --bin steamtron-desktop` passes.
- Data: `D:\SteamTronLibrary\steamtronapps\appmanifest_391220.json` contains
  `"launch_options": ["-nolauncher"]`, confirming the save path worked.
- Not run yet: installed-build launch of Rise of the Tomb Raider.

## Decisions and risks

- Game-level arguments precede target arguments. Steam and per-target launch
  options keep their existing order after that.
- Steam `%command%` launches are unchanged; they use Steam's own option string.
- The overlay wrapper still owns the merged drive and Proton construction; it
  now receives the combined arguments instead of only the target's.

## Links

- [0096](0096-windows-union-overlay.md) — Windows overlay launch.
- [0117](0117-fixes-launcher-overlay-lifetime.md) — overlay wrapper lifetime.
