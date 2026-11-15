# Open Store cards without waiting for the detail request

- Status: done
- Date: 2026-11-15
- Areas: `crates/steamtron-desktop/ui/app.js`

## Problem

Clicking a Store catalogue card appears to do nothing when the card's title is
not in the managed library. `openStoreItem` awaits `invoke("store_game")`
before setting `appView = "store-game"` and painting the detail page. The
Store catalogue can come entirely from the local cache, so this extra request
can go to the network and stall (or time out) even though every field needed
to paint the first detail view is already in the list entry. Until it settles
the screen does not change, so the click looks dead.

## Goal

Clicking any Store card paints that title's detail page immediately from the
list entry. The full `store_game` payload may refine the page afterwards but
must never gate navigation.

## Non-goals

- Changing the Store catalogue data source or caching.
- Redesigning the Store detail layout.
- Showing a loading spinner on the detail page.

## Requirements

- R1: A Store card click sets `appView = "store-game"` and renders the detail
  page synchronously from the cached list entry.
- R2: `store_game` is requested in the background; when it returns, the page
  refreshes only if the user is still viewing that same Store title.
- R3: A failed or slow `store_game` leaves the list-entry detail page intact.
- R4: Managed titles still open the library detail through
  `openManagedGame` as before.

## Acceptance criteria

- [x] Clicking a Store card shows `#store-back` immediately even when
  `store_game` is delayed.
- [x] When the delayed `store_game` resolves, the same detail page is
  refreshed without leaving the page.
- [x] Clicking a different card before the first request resolves does not
  overwrite the newer detail page.
- [x] Existing Store detail Escape-restore, focus, and pane navigation checks
  still pass.

## Verification

- `node --check crates/steamtron-desktop/ui/app.js` passed.
- jsdom harness `/tmp/steamtron-nav-test/panel-test.mjs` passed all 46 checks
  with `STORE_DELAY=3000 STORE_GAME_DELAYS="11:900,12:1800"`, including:
  - `Store detail paints before store_game responds`
  - `Background store_game response refreshes the detail page`
  - `Background store_game refresh keeps the user's focus`
  - `Late store_game response for the previous card is ignored`
  - `Escape returns a Store detail to its card`
- The base `/tmp/steamtron-nav-test/test.mjs` suite passed all 33 checks, and
  with `LIST_GAMES_FAIL=1`.
- Release build completed and the installed binary hash matched the build.

## Decisions and risks

- The first paint uses the cached catalogue entry, which may omit the
  description and genres; those appear when `store_game` resolves. This is
  preferable to a dead click when the network is unavailable.
- A page-identity check (`appView` plus `String(storeDetail.id)`) prevents a
  late response from a previous card replacing the current detail page.

## Links

- Implementation: `crates/steamtron-desktop/ui/app.js`
- Related: `docs/prd/0002-nav-focus.md`, `docs/prd/0006-store-cache.md`
