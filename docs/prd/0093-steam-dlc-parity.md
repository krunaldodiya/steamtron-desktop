# Match Steam DLC list and sizes

- Status: active
- Date: 2026-11-17
- Areas: `crates/steamtron-desktop/src/dlc.rs`,
  `crates/steamtron-desktop/src/main.rs`

## Problem

The DLC tab listed one DLC for Little Nightmares II while Steam listed two.
SteamTron used the public Store `appdetails` DLC array, which omitted the
Enhanced Edition even though the Lua registration and Steam PICS contain it.
The tab could also show DLC with no file size.

## Goal

Use Steam PICS depot metadata for the visible DLC list:

- a DLC appears only when it has a depot with a manifest size
- names come from the local SLSsteam DLC metadata
- sizes come from the same PICS manifest sizes Steam uses

## Non-goals

- Downloading DLC.
- Installing Store-only DLC that has no depot.
- Changing Lua registration.

## Requirements

- R1: Parse `picsbuffer_<base>.bin` from the SLSsteam cache.
- R2: Include only DLC IDs whose depot manifest total size is greater than zero.
- R3: Parse DLC names from `dlcmetadata_<base>.yaml` wire appinfo blocks.
- R4: Prefer the PICS list over the Store list when PICS is present.
- R5: Use PICS sizes in `dlc_sizes`.
- R6: Keep the Store fallback when no local PICS cache exists.

## Acceptance criteria

- [x] A zero-size DLC depot is excluded.
- [x] A named DLC from `dlcmetadata` is parsed.
- [x] Little Nightmares II reports Digital Content Bundle and Enhanced Edition.
- [x] Size values come from the PICS manifest sizes.
- [ ] Windows without a local PICS cache still uses the Store fallback; exact
  PICS parity on Windows is not implemented yet.

## Verification

- `cargo test -p steamtron-desktop dlc::tests` — 4 passed.
- `cargo check -p steamtron-desktop`.
- 0.1.23 Windows/Linux release builds succeeded.
- Cached DLC is returned immediately; the Store cache refresh runs on the Tauri
  async runtime and emits `dlc-catalogue-updated` for the open tab.

## Decisions and risks

- The local PICS cache is populated by SLSsteam Moon and is Linux-specific.
- A DLC without a depot manifest size is treated as not visible, matching the
  observed Steam behavior.

## Links

- `crates/steamtron-desktop/src/dlc.rs`
- `crates/steamtron-desktop/src/main.rs`
