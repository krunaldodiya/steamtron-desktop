# DLC sizes

- Status: done
- Date: 2026-11-16
- Areas: `crates/steamtron-desktop/{src/dlc.rs,src/main.rs,ui/app.js}`

## Problem

The DLC size column was hardcoded to `—`. Steam Store `appdetails` gives
names and release dates, but not install sizes.

## Implementation

- Steam PICS (`steamcmd +app_info_print <base_appid>`) is queried once per
  base game and DLC depot sizes are summed by `dlcappid`.
- If PICS has no DLC depot, fall back to
  `smd search <dlc_appid> --manifests 0 --json`.
- Results are cached in `~/.cache/steamtron/dlc-sizes.json`, including
  unknown (`0`).
- The DLC tab fills size cells asynchronously; names and release dates render
  immediately.

## Verification

- Lies of P DLC `1938420` resolved to `4,379,102,004` bytes.
- Little Nightmares PICS: DLC `601710` = `116,101,371` bytes, `584093` =
  `3,206,906` bytes.
- DLC App IDs that are packages without depots (for example Little Nightmares
  `584090`) have no PICS depot size and remain `—`.
- `node --check` passes.
- Desktop DLC tests pass.

## Decisions and risks

- First lookup for each DLC starts an SMD search and can take several seconds;
  cached entries are instant.
- Some DLC App IDs have no depot metadata in SMD (they are packages or share
  base-game depots). Those remain `—` and are cached as unknown.
- The UI starts with `—` instead of an indefinite `…` while sizes load.
