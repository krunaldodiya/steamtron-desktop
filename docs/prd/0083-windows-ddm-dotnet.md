# Windows downloads use DepotDownloaderMod + .NET

- Status: active
- Date: 2026-11-17
- Areas: `crates/steamtron-desktop/src/download.rs`, Windows runtime prerequisites

## Problem

SMD's native Rust engine remains at `Fetching depot manifests…` on Windows.
The Linux build already uses SMD's embedded DepotDownloaderMod engine for this
reason.

## Fix

- Set `use_native_downloader=false` on Windows as well.
- Require the Microsoft .NET runtime on Windows for the embedded DDM binary.
- Keep the VC++ runtime prerequisite.

## Verification

- Install VC++ x64 and .NET 8 x64 runtime in the Windows VM.
- Start a managed download; SMD emits depot progress rather than stalling.
