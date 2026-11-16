# DLC selection, queue, install, and removal

- Status: active
- Date: 2026-11-16
- Areas: `crates/steamtron-desktop/{src/dlc.rs,src/main.rs,src/download.rs,ui/app.js}`

## Requirements

- R1: DLC rows have enabled checkboxes when the DLC is present in the game
  manifest.
- R2: Checking one or more DLC queues them in the game's DLC state.
- R3: Play changes to Update while selected DLC is not installed.
- R4: Update opens the existing download manager and downloads queued DLC
  sequentially through SMD.
- R5: After completion, Play returns.
- R6: Unchecking installed DLC removes its files immediately; files overwritten
  by DLC are restored from a per-file backup.
- R7: DLC removal/download is blocked while the game is running.

## Implementation

- `.steamtron-dlc.json` stores selected and installed DLC.
- `.steamtron-dlc-backup/<appid>/` stores files overwritten by DLC.
- Each installed DLC records SHA-256 hashes for safe removal.
- For DLC listed as a depot in the base game's Steam PICS data, SMD downloads
  the selected depot IDs from the base Lua into staging. This is required for
  DLC App IDs that SMD cannot resolve directly (for example Little Nightmares
  601710).
- Otherwise SMD downloads the DLC App ID directly.
- Files are copied into the game root and recorded.
- `.smd-dlcs.json` is updated with installed DLC IDs for Goldberg ownership.

## Verification

- SMD downloaded a Lies of P soundtrack depot into a staging folder
  (340 MB class) successfully.
- SMD downloaded Little Nightmares DLC `601710` from the base Lua with
  `--depots 601710` into staging; it produced the OST folder and files.
- Real-game selection toggle: selected then deselected 1938420 in Lies of P;
  state file returned to `selected: []`.
- `installs_and_records_downloaded_dlc_files` passes.
- `uninstall_restores_a_file_that_the_dlc_overwrote` passes.
- Desktop tests: 19 passed.
- UI syntax check passes.
- The full checkbox → Update → download manager → Play cycle was not exercised
  through the installed GUI in this session.

## Decisions and risks

- DLC is downloaded per App ID into a staging folder, then merged into the game
  root. This avoids redownloading the base game.
- Large DLC downloads are sequential.
- The existing download manager consumes emitted `download-progress` events;
  cancellation of an in-flight SMD DLC process is not yet wired.
