# Storage settings prefetch and non-blocking render

- Status: active
- Date: 2026-11-24
- Areas: `steamtron-desktop` UI settings, `available_library_drives`

## Problem

Opening **Settings → Storage** replaces the whole settings body with
`Loading…` and waits for two Tauri calls:

- `library_snapshot`
- `available_library_drives`

On Windows, `available_library_drives` enumerates logical drives, checks
`GetDriveTypeW`, `is_dir`, canonicalization, free space and writability. A
slow removable or disconnected drive can hold that call for several seconds;
the current code waits up to 4 s per call and shows nothing useful meanwhile.

## Goal

Storage opens immediately from data prefetched at launch. If the drive list is
still being refreshed, only the drive rows show a loading state; the rest of
the page stays usable.

## Non-goals

- No change to which drives are listed or how they are registered.
- No background polling loop; a one-shot launch prefetch and explicit refresh
  are enough.
- No change to General/Proton settings loading.

## Requirements

- R1: Prefetch the library snapshot and available drive list after sign-in,
  in parallel with the initial Home load.
- R2: Cache both results in the frontend; share a single in-flight drive
  enumeration instead of starting a new one.
- R3: Render Storage from cache without awaiting a fresh call.
- R4: If drives are not ready yet, render the page and show `Loading drives…`
  only in the drive-row area, then fill rows when enumeration finishes.
- R5: Invalidate the cache after add/remove/default-drive changes and on the
  **Refresh drives** button.
- R6: Reduce the fallback timeout from 4 s to 1.5 s when no cache exists.

## Acceptance criteria

- [x] The library/drives prefetch starts during boot and after sign-in.
- [x] Storage uses the cached values and renders without a whole-page spinner.
- [x] A pending drive enumeration re-renders only when it completes.
- [x] Drive cache invalidation is wired to add/remove/default/refresh.
- [x] `node --check` passes on `app.js`.
- [x] Runtime: Storage opened with drive/data content after launch without
      the previous whole-pane `Loading…` state (0.1.68).

## Verification

- `node --check crates/steamtron-desktop/ui/app.js` — passed.
- `cargo check --workspace` — passed.
- Runtime verification is pending until 0.1.68 is installed.

## Decisions and risks

- The cache is memory-only; restarting the app re-primes it.
- A drive list can change outside the app (USB insertion); the explicit
  **Refresh drives** button invalidates and re-enumerates.
- `available_library_drives` remains synchronous in Rust; the UI no longer
  blocks the whole settings body on it.

## Links

- `docs/prd/0142-storage-data-folder.md`
