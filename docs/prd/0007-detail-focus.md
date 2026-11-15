# One visible focus and detail-page action focus

- Status: done
- Date: 2026-11-15
- Areas: `crates/steamtron-desktop/ui/app.js`,
  `crates/steamtron-desktop/ui/style.css`

## Problem

Two focus indicators could be visible at once. `.cover-card.active` and
`.cover-card.nav-focus` used the same accent border, so pressing Down from the
Home menu focused the hero Play button while the selected card kept a second
outline. The same overlap appeared in Home, Library, and Store rails.

Opening a managed game detail did not move focus to its primary action. The
header could keep the outline instead, even though the detail page exposes
Play, Download, or Stop. A Store catalogue entry that is not in the library
opens a Store detail with "Add to library", and that page did not focus the
button either.

## Goal

Exactly one element shows keyboard/controller focus at a time. Opening any game
detail lands on its primary action: Play, Download, Stop, or Add to library.

## Non-goals

- Removing the remembered active-card state used for route restoration.
- Changing the top-nav route highlight.
- Adding new actions to the detail pages.

## Requirements

- R1: The selected/active card must not draw a focus ring unless it also has
  `.nav-focus`.
- R2: Only `.nav-focus` draws the accent border for cover cards.
- R3: Opening a managed game detail focuses Play when enabled, otherwise
  Download or Stop, otherwise Back.
- R4: Opening a Store detail focuses Add to library when available, otherwise
  Back.
- R5: Escape still restores the card that opened the detail.

## Acceptance criteria

- [x] While the Home hero Play button is focused, no cover card also shows a
  focus ring.
- [x] Opening a Home or Library managed detail focuses `#managed-play`.
- [x] Opening a Store detail for a title with a Steam App ID focuses
  `#store-add`.
- [x] A detail page has exactly one `.nav-focus` element.
- [x] Escape from Home, Library, and Store details restores the originating
  card.

## Verification

- jsdom harness `/tmp/steamtron-nav-test/test.mjs` passed all 33 checks,
  including:
  `The selected card does not keep a second focus ring while Play is focused`,
  `Opening a Home detail focuses Play`,
  `Opening a Library detail focuses Play`,
  `Opening a Store detail focuses Add to library`, and
  `Only the detail action carries focus styling`.
- The harness passed with `STORE_CACHE_MISS=1` and `LIST_GAMES_FAIL=1`.
- Live build: only the focused Home card drew the accent border while Play was
  focused. Store detail focus is covered by the harness because the live machine
  has no local Steam installation to open a managed detail from the Store.
- `node --check crates/steamtron-desktop/ui/app.js` passed.

## Decisions and risks

- `.active` remains the route's remembered card for restoration and paging
  logic; it is no longer a visual focus style.
- Managed detail focus prefers Play only when the button is not disabled, so an
  uninstalled game lands on Download instead.

## Links

- Related: `docs/prd/0002-nav-focus.md`, `docs/prd/0005-route-focus.md`,
  `docs/prd/0006-store-cache.md`
