# Prevent Store flash on app launch

- Status: active
- Date: 2026-11-17
- Areas: `crates/steamtron-desktop/ui/app.js`

## Problem

The app launches on Home, but the Store page appears briefly in the detail area
before Home is painted. This happens because the background Store warm-up calls
`renderStorePage()` whenever a cached page is painted, regardless of the active
route.

## Fix

Only paint the Store from the background warm-up when the active route is
`store`. Explicit Store navigation still renders immediately.

## Verification

- Launch the app with a warm Store cache; Home stays selected and no Store page
  appears.
- Open Store; cached catalogue renders.
