# SMD manifest fallback completes in verbose mode

- Status: done
- Date: 2026-11-15
- Areas: `crates/steamtron-desktop/src/download.rs`

## Problem

Managed downloads intermittently stopped at `Fetching manifest directly from
Steam CDN` with:

```
Steam direct failed (chunk decode failed: unrecognised chunk container magic: 50 4B).
Falling back to configured manifest source.
All sources failed (...). No ManifestHub API key set — add one and retry.
```

The same Lua/App ID installs smoothly in Steam via steam-moon. Running SMD 1.5.0
manually with the exact app command reproduced the failure, but adding
`--verbose` (`-v`) made the same manifest resolve via steam.run and decode into
49 chunks immediately. The difference is where SMD writes backend diagnostics:
without `-v` it writes them to the shared data-dir log, and the manifest-code
fallback stalls.

## Goal

Managed downloads use the SMD invocation that reliably completes the
manifest-code fallback, and a real SMD failure is surfaced instead of the generic
"Download stopped unexpectedly" message.

## Non-goals

- Changing SMD source code or the SMD binary.
- Shipping a ManifestHub API key.
- Replacing SMD with the Steam client downloader.

## Requirements

- R1: Invoke SMD with `--json -v`; JSON remains on stdout and diagnostics go to
  the separate stderr log.
- R2: Do not merge stderr into the JSON log.
- R3: When status detects a dead process, retain an existing specific SMD error
  message; use the generic message only when no useful message exists.

## Acceptance criteria

- [ ] A download whose direct Steam manifest request is AccessDenied resolves
  through the external manifest-code provider and starts downloading.
- [ ] JSON progress parsing still works with `-v`.
- [ ] A failed download shows the SMD error, not only "stopped unexpectedly".

## Verification

- Exact SMD command without `-v`: stalled at `Fetching manifest directly from
  Steam CDN`; after 70s it reported the chunk-magic error.
- Same command with `-v`: logged `resolved depot 113201 via steam.run`, decoded
  49 chunks, and built the chunk queue.
- App command updated to `--json -v`; stderr already goes to
  `download-<stamp>.stderr.log`.
- Installed-app run on App 113200 logged `resolved depot 113201 via steam.run`
  and reached DepotDownloaderMod.

## Decisions and risks

- This is a workaround for an SMD 1.5.0 diagnostic-logging bug. If upstream fixes
  it, the flag can stay: verbose diagnostics are useful and do not pollute JSON.
- Steam direct manifest codes can be AccessDenied anonymously for some depots;
  steam.run is the fallback. A ManifestHub key remains an optional future
  setting.

## Links

- Related PRD: `docs/prd/0027-background-downloads.md`
- SMD: https://github.com/mcbabel/Steam-Manifest-Downloader
