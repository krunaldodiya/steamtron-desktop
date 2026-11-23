# Aggregate download progress across all depots

- Status: active
- Date: 2026-11-23
- Areas: `steamtron-desktop` download state and Store UI

## Problem

Max Payne 3 is a **32.5 GiB** download. The download manager showed:

- `Downloaded 100.0 MiB`, `Total size 100.0 MiB`, and **100.0%** at the top;
- `Content groups 2 / 3 complete` with depot 204104 still at 5.3%;
- the game page also showed `100.0% · 100.0 MiB of 100.0 MiB`.

The parser writes the console line's **downloaded bytes** into `depot_totals`, so one 100 MiB depot becomes the whole total. Each `plan` event also replaced the depot order/plan total instead of merging, and the progress could reach 100% before every depot finished. The expected size (the UI already knows 32.5 GiB via `steam_download_size`) was never passed to the download state.

## Goal

The total is the expected download size or the sum of every depot, and 100% is shown only after every depot completes.

## Requirements

- R1: Pass the expected Steam download size from the UI to `start_managed_download`; fall back to the appmanifest `details.size_bytes` and then the merged depot totals.
- R2: Merge `plan` depots into the known depot map instead of replacing it; keep the maximum size per depot.
- R3: Never treat the `Downloaded …` console bytes as a depot total; they only update completed bytes.
- R4: `total = max(expected_total, sum(depot totals))`; completed = sum of depot bytes.
- R5: If `depots_done < depots_total`, cap the overall percent below 100.
- R6: Persist the expected total in `active-download.json` so a reconnect after an app restart keeps the correct size.
- R7: Apply to both the modal and the game-page progress (same `DownloadState`).

## Acceptance criteria

- [x] Code no longer writes downloaded bytes into depot totals.
- [x] Plans merge; expected size is accepted and persisted.
- [x] Percent is capped while depots are incomplete.
- [ ] Max Payne 3 shows 32.5 GiB total, not 100 MiB, after a fresh download start.
- [ ] Overall progress stays below 100% until 3/3 content groups complete.

## Verification

- Code: `cargo check -p steamtron-desktop --bin steamtron-desktop` passes.
- Code: `node --check crates/steamtron-desktop/ui/app.js` passes.
- Not run yet: installed-build download (0.1.52).

## Decisions and risks

- The expected size is a display denominator, not a validation target; if the depot sum grows beyond it, the larger value wins.
- A stalled downloader can still leave the UI at its last known byte count; the percent cap prevents a false 100%, but the stall itself needs a cancel/retry.
- The existing resume sidecar gains `expected_total` with a serde default so old sidecars keep working.
