# Linux overlay must carry the game's launch environment

- Status: active
- Date: 2026-11-19
- Areas: `crates/steamtron-desktop/src/main.rs`

## Problem

A fixes pack that works on Windows crashed on Linux immediately after the
splash, even though the same GoldBerg files were present in the fixes folder.
When fixes exist, `play_managed` launches a helper (`--steamtron-overlay`) that
mounts the union and starts Proton itself. That helper command was built
without `plan.environment`, so the game's
`WINEDLLOVERRIDES=steam_api64=n,b` and `PROTON_DISABLE_LSTEAMCLIENT=1` never
reached Proton. Proton then used its own Steam client integration instead of
the fixes DLL. On Windows the same DLL is loaded natively, so the override is
not needed and the game worked.

## Requirements

- R1: The overlay helper inherits the game's launch environment on both
  platforms.
- R2: The helper's existing override merge for Uplay loader DLLs continues to
  work.
- R3: A Steam-required game still drops `PROTON_DISABLE_LSTEAMCLIENT` and the
  `lsteamclient` override before launch.

## Acceptance criteria

- [x] `play_managed` applies `plan.environment` to the overlay helper.
- [ ] Linux runtime: launch a GoldBerg fixes pack under Proton and reach
  gameplay. Not testable on this Windows host.

## Verification

- `cargo check -p steamtron-desktop` passes.
- Code inspection: the non-overlay path used `plan.command()`, which applies
  `plan.environment`; the overlay path built a fresh command and did not.
- User's backup manifest for 424840 records both variables, proving they were
  configured and then lost by the overlay wrapper.

## Risks

- Passing `STEAM_COMPAT_DATA_PATH`/`STEAM_COMPAT_CLIENT_INSTALL_PATH` to the
  helper duplicates values the wrapper sets itself; identical values, so no
  behaviour change.
