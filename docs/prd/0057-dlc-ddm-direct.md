# Direct DepotDownloaderMod for DLC

- Status: active
- Date: 2026-11-16
- Areas: `crates/steamtron-desktop/src/{dlc.rs,main.rs}`

## Problem

SMD could not resolve some DLC App IDs directly:

```text
error: App 601710 not found in any configured depot source
```

The DLC is a depot in the base game's Steam PICS data, not a standalone SMD
source.

## R&D comparison

- SMD: convenient source resolution and progress JSON, but DLC App ID lookup can
  fail and it adds a process/metadata layer.
- DepotDownloaderMod: already bundled with SteamTron, requires explicit
  app/depot/manifest/key inputs, and downloads the same depot directly.
- SteamCMD: can query PICS and download depots, but needs login and is heavier.
- Custom Rust Steam CDN downloader: most control, highest implementation and
  maintenance cost; not justified now.

Measured download of Little Nightmares depot `601710`:

- SMD base-Lua route: about 18 seconds
- Direct DDM: about 11 seconds

## Implementation

- PICS maps DLC App IDs to depot IDs.
- For mapped DLC, write its manifest and key from the base bundle and run DDM
  directly with `-app <base> -depot <depot> -manifest <gid>`.
- Fall back to SMD for DLC without a PICS depot mapping.
- Skip `.DepotDownloader` metadata when merging files.

## Verification

- Manual DDM download of `601710` produced `Little Nightmares OST` and its
  MP3 files.
- PICS parser maps `601710` to depot `601710`.
- DLC size cache is instant on a complete cache (853 µs).
- DLC tests and desktop tests pass.

## Risks

- The direct DDM path requires a consistent base Lua manifest/key bundle.
- The full UI checkbox → Update → Play path is still active/pending verification.
