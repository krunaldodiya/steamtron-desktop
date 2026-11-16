# Faster backup archives with fdatasync

- Status: done
- Date: 2026-11-16
- Areas: `crates/steamtron-desktop/src/main.rs`,
  `crates/steamtron-library/src/backup.rs`

## Problem

A 48.5 MB Isaac backup took 17 seconds. The activity-line hypothesis was
testable: the archive function was benchmarked with a no-op progress emitter,
then compared with the filesystem directly.

## Measurements

- Raw `tar` of 47 MB to the Samsung NTFS3 mount: **11.9 s**
- `dd conv=fsync` 47 MB to the same mount: **16.5 s**
- 47 MB with `fdatasync`: **4.1 s**
- SteamTron `archive_component` with `sync_all`: **20.4 s**
- SteamTron `archive_component` with `sync_data`: **4.05 s**

Activity emission was not the cause.

## Requirements

- R1: Flush backup archive data with `fdatasync` instead of `fsync`.
- R2: Apply the same change to export ZIPs.
- R3: Keep progress/activity behavior unchanged.

## Verification

- Actual release-mode archive benchmark on the Isaac folder:
  `archive elapsed=4.051536249s bytes=48539029`.
- Desktop tests pass.
- Library tests pass.

## Decisions and risks

- `fdatasync` flushes file data and the metadata needed to retrieve it, without
  syncing unrelated inode timestamps. It is appropriate for completed archive
  files.
- The destination directory entry itself is still subject to normal OS flush
  semantics.
