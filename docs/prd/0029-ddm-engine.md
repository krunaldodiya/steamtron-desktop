# Use DepotDownloaderMod instead of SMD's native engine

- Status: active
- Date: 2026-11-15
- Areas: `crates/steamtron-desktop/src/download.rs`

## Problem

After the manifest-code fallback succeeds, SMD 1.5.0's native Rust downloader
builds its chunk queue and then stalls indefinitely:

```
[native] depot 113201 manifest decoded: 49 chunks, 0.05 GiB total uncompressed
[native] depot 113201 chunk task queue built; awaiting completion
```

No bytes arrive, the manager stays at 0%, and SMD retries the manifest. The same
App ID installed through Steam/steam-moon works normally.

## Goal

Use the downloader engine that completes real downloads reliably while keeping
the existing Lua/manifest flow, progress events, pause/resume, and persistence.

## Non-goals

- Shipping Valve SteamCMD as the downloader.
- Requiring Steam client login or ownership for managed games.
- Reimplementing a SteamKit downloader in Rust.

## Requirements

- R1: Configure SMD with `use_native_downloader=false` so downloads use the
  embedded DepotDownloaderMod engine.
- R2: Keep the Lua manifest, `--like-steam`, platform/language, and JSON events
  unchanged.
- R3: Pause/resume/cancel still signal the SMD process group, covering its
  DepotDownloaderMod child.
- R4: Persistent logs and restore keep working with the DDM output stream.

## Acceptance criteria

- [ ] The Binding of Isaac downloads real bytes and completes with DDM.
- [ ] A successful DDM result with `"error": null` is treated as success.
- [ ] The manager shows progress and the final installed state.
- [ ] Pause/resume/cancel and app-exit reconnect still work.

## Verification

- Manual legacy-engine run:
  `smd ... download 113200.smd.lua --like-steam --platform windows
  --language english --json -v` with `use_native_downloader=false`.
- Result: anonymous Steam3 login, depot 113201 preallocated, 46,721,552 bytes
  downloaded, `depot_complete`, then
  `Download complete. 1/1 depots downloaded successfully.` in ~14 seconds.
- Native engine on the same manifest stalled at `awaiting completion` with no
  data.
- App DDM run reached 100% but was marked failed: SMD's completion result uses
  `"error": null`, and `Option::is_none()` on the present JSON key returned
  false. Completion now accepts a null error, and completed depots report 100%.

## Decisions and risks

- DepotDownloaderMod is not Valve-official, but it is the SteamKit-based engine
  SMD itself ships and the only option tested that works without a Steam login
  or a ManifestHub key for this App.
- Valve's SteamCMD is official and stable, but anonymous login does not grant
  ordinary game downloads; using it would require Steam credentials and would
  not fit the current Lua/steam-moon ownership flow.
- The Steam client downloader is official and works with steam-moon, but it
  requires Steam registration/ownership and would move download/finalization
  outside SteamTron's managed overlay and prefix flow.
- DDM output is parsed through SMD's JSON events; byte totals remain
  unavailable until it reports depot completion, so percentage may be coarse for
  some titles.

## Links

- Related PRDs: `docs/prd/0027-background-downloads.md`,
  `docs/prd/0028-smd-manifest-fallback.md`
- SMD: https://github.com/mcbabel/Steam-Manifest-Downloader
- DepotDownloader: https://github.com/SteamRE/DepotDownloader
