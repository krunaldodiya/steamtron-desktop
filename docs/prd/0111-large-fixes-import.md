# Import large fixes archives

- Status: done
- Date: 2026-11-19
- Areas: `crates/steamtron-core/src/fixes.rs`

## Problem

Importing the lua.tools Assassin's Creed Origins fix (`582160.zip`,
1,280,322,599 bytes) failed with **"fixes ZIP is invalid: archive is too large"**.
The importer capped archives at 512 MiB and extracted content at 2 GiB. Full
fix packs can include the cracked executable and forge data, so they exceed both.

For AC Origins, the large pack is required: the four small loader files alone
fail under WinFsp with `0xC0000139` (entry point `UPLAY_PRODUCT_GetProductList`
missing), while the full pack (cracked `ACOrigins.exe`, `DataPC.forge`,
`DataPC_Extra.forge`, shader/streaming data) runs through the overlay.

## Goal

Import full-size fixes packs without an artificial archive-size rejection.

## Requirements

- R1: Raise the archive limit to 64 GiB.
- R2: Raise the extracted-size limit to 64 GiB.
- R3: Keep the 20,000-file cap and path/encryption safety checks.

## Acceptance criteria

- [x] A 1.28 GB archive no longer hits the size check.
- [x] Import completes and the Fixes tab lists the full pack.
- [x] AC Origins starts through the overlay with the imported pack.

## Verification

- Constants changed to 64 GiB; `cargo test -p steamtron-core fixes` passes.
- Full pack extracted manually and tested as the overlay source: the game
  stayed running past 15 s (overlay log shows the merged path, no exit).
- 0.1.33 import via `install_managed_fixes` with the file path returned 10
  files at `D:\SteamTronLibrary\steamtronapps\fixes\Assassin's Creed Origins`;
  the overlay run stayed alive past 15 s with `merged=S:\`.

## Decisions and risks

- A fixed 64 GiB ceiling still prevents unintentional disk exhaustion/zip bombs;
  the extractor checks the declared total as it writes.
- Large imports take time and disk space (about 1.4 GB for AC Origins).

## Links

- Related: `0110-unlink-keeps-library-entry`.
