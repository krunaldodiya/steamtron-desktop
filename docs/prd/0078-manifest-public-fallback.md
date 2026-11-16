# Public manifest fallback

- Status: active
- Date: 2026-11-17
- Areas: `crates/steamtron-library/src/lib.rs`

## Problem

Add to library fails with `manifest download failed: HTTP 404 Not Found` on
both Linux and Windows because the signed-in manifest API endpoint is absent
or does not have the App ID. The public SteamML archive has an available ZIP.

## Requirements

- R1: Keep the signed-in API as the preferred source.
- R2: If the API fails, try the documented public SteamML ZIP.
- R3: Validate the archive and cache it exactly like the API bundle.
- R4: If both sources fail, show both errors.
- R5: Do not change Game Link or local ZIP import.

## Verification

- Public ZIP check for App IDs 620 and 250900.
- Add to library on Linux and Windows.
