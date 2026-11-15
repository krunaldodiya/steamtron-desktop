# Store pane isolation and route-tab Up focus

- Status: done
- Date: 2026-11-15
- Areas: `crates/steamtron-desktop/ui/app.js`

## Problem

Two navigation leaks remain after the route-focus work:

1. Moving Up out of a route body lands on the Home tab instead of the tab of
   the screen being viewed. With Library open, Up from the first card focuses
   `#nav-home`; Store behaves the same. Focus is picked by spatial distance
   across every control in the document, and Home is the nearest tab to the
   leftmost card.
2. The Store's filter sidebar and catalogue grid are treated as one region for
   Up/Down. At the bottom of the grid, Down has no catalogue candidate, falls
   back to the whole document, and jumps into the sidebar. The sidebar leaks
   the same way in reverse. Down from the Store tab while the catalogue is
   loading also falls through to `#store-search` in the sidebar.

## Goal

Up/Down stay inside the panel focus is in, and Up from a route body reaches
that route's own top-nav tab. Down from the Store tab reaches the first
catalogue card, never the filter sidebar.

## Non-goals

- Changing the Home menu → hero Play → rail chain or Home rail wrap.
- Changing the Store layout, filters, or catalogue data source.
- Persisting focus across restarts.

## Requirements

- R1: Up from route content with no control above it focuses the current
  route's tab (`#nav-library`, `#nav-store`, `#nav-home`), not a sibling tab.
- R2: Store Up/Down never cross between `.store-sidebar` and `.store-content`;
  at the top or bottom of either pane focus stays there.
- R3: Store Left/Right is the only pane switch: Right from the sidebar focuses
  catalogue content, Left from the catalogue focuses the remembered sidebar
  control. Left/Right inside the catalogue still walks the grid.
- R4: Down from `#nav-store` focuses the first catalogue card; while the
  catalogue is still loading the request is deferred to the first painted
  card instead of focusing `#store-search`.
- R5: A deferred Store focus never steals focus after the user has left Store.

## Acceptance criteria

- [x] Library entry, then Up: `document.activeElement` is `#nav-library`.
- [x] Store entry, then Up from the first card: active element is
  `#nav-store`.
- [x] Store sidebar bottom item, then Down: focus stays in the sidebar.
- [x] Store catalogue last pager control, then Down: focus stays in
  `.store-content`.
- [x] Store sidebar focus, then Right: focus moves to a catalogue card.
- [x] Store catalogue card, then Left: focus moves to the sidebar.
- [x] Store catalogue card in a non-leftmost column, then Left: focus moves to
  the card on its left.
- [x] Down from `#nav-store` during loading focuses the first card once it
  paints, with no intermediate focus on `#store-search`.
- [x] Existing Home/Library/Store focus, wrap, and detail-restore checks still
  pass.

## Verification

- `node --check crates/steamtron-desktop/ui/app.js` passed.
- jsdom harness `/tmp/steamtron-nav-test/panel-test.mjs` passed all 42
  checks with separate sidebar/grid geometry and `STORE_DELAY=3000`:
  - `Up from a Library card focuses the Library tab`
  - `Up from a Store card focuses the Store tab`
  - `Down at the bottom of the Store sidebar stays in the sidebar`
  - `Down at the bottom of the Store catalogue stays in the catalogue`
  - `Left inside the Store catalogue moves one card left`
  - `Left at the catalogue edge focuses the Store sidebar`
  - `Right from the Store sidebar focuses the first catalogue card`
  - `Down from the Store tab during loading keeps the tab focused`
  - `Deferred Store focus lands on the first catalogue card`
- The original `/tmp/steamtron-nav-test/test.mjs` suite still passed all 33
  checks (Home chain, rail wrap, route selection memory, detail restore).
- Release build completed and the installed binary hash matched the build.

## Decisions and risks

- Up/Down use only same-region candidates. The previous whole-document
  fallback is removed; route tabs are reached explicitly through
  `activeNavControl()`.
- Store pane crossing is explicit on Left/Right, keeping horizontal grid
  movement inside the catalogue.

## Links

- Implementation: `crates/steamtron-desktop/ui/app.js`
- Related: `docs/prd/0002-nav-focus.md`, `docs/prd/0005-route-focus.md`
