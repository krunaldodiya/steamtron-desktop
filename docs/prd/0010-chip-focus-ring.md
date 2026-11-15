# One focus ring on pill chips

- Status: done
- Date: 2026-11-15
- Areas: `crates/steamtron-desktop/ui/style.css`

## Problem

A focused "Recently viewed" Store chip draws two rounded borders: the chip's
own 1px border plus the global `.nav-focus` outline with a 2px offset. The gap
between them reads as a double ring instead of one focus indicator.

After collapsing the rings, the remaining 2px outline was clipped at the top
edge of `.store-content`: the chip row starts flush with the scroll container,
so the outline extends above the scrollport and loses its top edge.

## Goal

A focused pill chip shows exactly one accent ring.

## Non-goals

- Changing chip size, padding, colour, or layout.
- Changing focus styling for cards, tabs, or inputs.

## Requirements

- R1: `.store-chip.nav-focus` and `button.game-chip.nav-focus` draw a single
  2px accent ring flush with the pill edge.
- R2: Unfocused and hovered chips keep their existing 1px border.
- R3: Other `.nav-focus` controls keep the global 2px offset outline.
- R4: The focused chip's ring is fully visible when the chip row is at the top
  of the scrolling Store catalogue.

## Acceptance criteria

- [x] A focused Store chip renders one accent ring with no inner border.
- [x] A focused game-detail chip renders the same single ring.
- [x] Unfocused and hovered chips are unchanged.
- [x] The focused chip's ring is complete on all sides at the top of the
  Store catalogue.

## Verification

- Headless Chrome rendered a fixture using the real `tokens.css` and
  `style.css` with `.store-chip.nav-focus` and
  `button.game-chip.nav-focus`; the screenshot shows one accent ring on each
  focused chip and the normal 1px border on the unfocused chips.
- The chip rule only overrides focused chips, so hover and idle styles are
  untouched.
- Headless Chrome rendered the chip row as the first child of a scrolling
  `.store-content`; before the change the top of the ring was clipped, and
  after adding 4px of top padding to `.store-recent` the ring is complete.

## Decisions and risks

- The chip keeps its transparent 1px border for layout stability; the outline
  sits on the border edge (`outline-offset: 0`) so the two rings coincide.
- The clipping fix reserves 4px above the chip row only when a recently-viewed
  row exists, so the catalogue grid does not shift when there is no such row.

## Links

- Implementation: `crates/steamtron-desktop/ui/style.css`
- Related: `docs/prd/0002-nav-focus.md`
