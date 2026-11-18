# Windows artwork asset scope

- Status: done
- Date: 2026-11-18
- Areas: `crates/steamtron-desktop/tauri.conf.json`

## Problem

Local artwork never loaded on Windows: covers replaced the provisional image
with a broken image, and the detail hero stayed blank. Both worked on Linux.

`convertFileSrc` produced
`http://asset.localhost/C%3A%5CUsers%5C…%5Csteamtron%5Cartwork%5C3468650_cover.jpg`,
and a fetch of that URL returned **403**. The real directory is
`%LOCALAPPDATA%\steamtron\artwork`, but Tauri scope variables do not include
`$LOCALAPPDATA`; the supported variable is `$LOCALDATA`. The Windows scope entry
was therefore unresolved and the asset protocol rejected the file. On Linux the
`$HOME/.local/share/...` entry matched, so local artwork loaded there.

For Cricket 26 the Steam CDN fallback (`header.jpg`) is also missing, so the card
showed a broken image instead of the local cover; the CDN-only games still
looked fine because their remote images load.

## Goal

Local cover and hero images load from `%LOCALAPPDATA%\steamtron\artwork` on
Windows, matching Linux behavior.

## Requirements

- R1: Scope the asset protocol with `$LOCALDATA/steamtron/artwork/**`, the
  variable Tauri resolves on Windows and Linux.
- R2: Keep one entry that works on both platforms.
- R3: Do not change how artwork paths are produced or cached.

## Acceptance criteria

- [x] Fetching the `convertFileSrc` cover URL returns 200 on Windows.
- [x] The Cricket 26 card shows the local cover, and the detail hero shows the
  local hero.

## Verification

- CDP before the fix: asset URL status 403, `naturalWidth` 0.
- After the rebuilt install: cover and hero fetches returned 200; an `<img>`
  probe reported `naturalWidth` 600 for the cover and 3840 for the hero; both
  Cricket cards used the `asset.localhost` cover URL.

## Decisions and risks

- `$LOCALDATA` maps to `%LOCALAPPDATA%` on Windows and `$XDG_DATA_HOME` on
  Linux, which is exactly the base of `Layout::home()`.
- The scope stays limited to `artwork/**`, not the whole data folder.

## Links

- Related: `0100-restored-artwork`.
