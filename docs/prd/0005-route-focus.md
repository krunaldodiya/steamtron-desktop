# Independent route focus and Home rail looping

- Status: done
- Date: 2026-11-15
- Areas: `crates/steamtron-desktop/ui/app.js`

## Problem

Four keyboard/controller navigation problems remained after the first focus
fixes:

1. Entering a top-nav route did not always land on that route's own first card;
   the active-card lookup could resolve to a card selected elsewhere.
2. Home and Library shared one `managedSelectedId`. Focusing a Home card such
   as Cricket 26 made the same card active in Library, so switching tabs carried
   the selection across screens instead of each screen keeping its own focus.
3. Only the "Recently played" rail wrapped. At the end of either Home rail,
   focus escaped to another part of the page instead of looping to the rail's
   first card.
4. Home vertical movement skipped the hero: Down from the Home menu jumped
   straight to a card instead of visiting the hero Play button, and Up from the
   first rail did not return through Play to the menu.

## Goal

Every route owns its focus and selection. Home and Library are independent.
Both Home rails loop horizontally. Home moves vertically in the order
top menu → hero Play → first card, and the reverse when going up.

## Non-goals

- Changing the top-nav order or the Store route's layout.
- Persisting focus across application restarts.
- Changing where the hero art or Play action comes from.

## Requirements

- R1: Entering Home, Library, or Store focuses that route's remembered card, or
  its first card when the route has no remembered focus.
- R2: Home and Library keep separate selected/focused cards; moving focus in one
  must not change the other.
- R3: Left and Right wrap at both ends of both Home rails.
- R4: Home Down/Up follows top menu → hero Play → first card and the reverse.
  Up from a lower rail still moves to the rail above it.

## Acceptance criteria

- [x] Home starts on its first card; Library and Store start on their own first
  cards when visited for the first time.
- [x] With Home on game1 and Library on game2, switching routes restores each
  route's own card and active outline.
- [x] Right on the last recent/setup card wraps to that rail's first card; Left
  on the first card wraps to that rail's last card.
- [x] Down from the Home menu focuses `#home-play`; Down from Play focuses the
  Home selection; Up from the first rail focuses Play; Up from Play focuses
  `#nav-home`.
- [x] Escape from a Home or Library detail still restores the card that opened
  it.

## Verification

- jsdom harness `/tmp/steamtron-nav-test/test.mjs` passed all 33 checks, with
  geometry that separates the top menu, hero actions, and both Home rails:
  boot focus, route-independent selection, both-rail wrap in both directions,
  the full Home vertical chain, detail Escape restore for Home/Library/Store,
  and the account-dialog Escape check.
- The same suite passed with `LIST_GAMES_FAIL=1`, proving the legacy Steam scan
  failure path does not change navigation.
- `node --check crates/steamtron-desktop/ui/app.js` passed.
- Release build completed, and the installed binary hash matched the build
  hash.

## Decisions and risks

- `managedSelectedId` is now only the detail-page selection. Home keeps
  `{ id, section }` in `routeSelection.home`, Library keeps `{ id }` in
  `routeSelection.library`, and card markup reads only its own route.
- Route entry uses the route's remembered card with a first-card fallback, so a
  new screen never inherits another screen's card.
- Home vertical anchors are explicit rather than relying only on spatial
  scoring; lower-rail movement still uses the spatial fallback so Up from the
  setup rail reaches the recent rail above it.

## Links

- Related: `docs/prd/0002-nav-focus.md`, `docs/prd/0004-default-home.md`
