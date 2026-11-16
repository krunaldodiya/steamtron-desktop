# Windows SMD native downloader

- Status: active
- Date: 2026-11-17
- Areas: `crates/steamtron-desktop/src/download.rs`

## Problem

On Windows, managed downloads stop at `Preparing depots…`. `configure_smd`
forces SMD's DepotDownloaderMod engine off for Linux stability, but
DepotDownloaderMod is not provisioned on Windows. SMD exits before emitting
depot progress.

## Fix

Enable SMD's self-contained native downloader on Windows; keep the
DepotDownloaderMod path on Linux.

## Verification

- Windows VM: add The Binding of Isaac and start download.
- Confirm SMD emits depot events and bytes increase.
- Confirm Linux download behavior is unchanged.
