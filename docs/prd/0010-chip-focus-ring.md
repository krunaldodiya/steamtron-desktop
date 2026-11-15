# One focus ring on pill chips

- Status: done
- Date: 2026-11-15
- Areas: `crates/steamtron-desktop/ui/style.css`

## Problem

A focused "Recently viewed" Store chip draws two rounded borders: the chip's
own 1px border plus the global `.nav-focus` outline with a 2px offset. The gap
between them reads as a double ring instead of one focus indicator.

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

## Acceptance criteria

- [x] A focused Store chip renders one accent ring with no inner border.
- [x] A focused game-detail chip renders the same single ring.
- [x] Unfocused and hovered chips are unchanged.

## Verification

- Headless Chrome rendered a fixture using the real `tokens.css` and
  `style.css` with `.store-chip.nav-focus` and
  `button.game-chip.nav-focus`; the screenshot shows one accent ring on each
  focused chip and the normal 1px border on the unfocused chips.
- The chip rule only overrides focused chips, so hover and idle styles are
  untouched.

## Decisions and risks

- The chip keeps its transparent 1px border for layout stability; the outline
  sits on the border edge (`outline-offset: 0`) so the two rings coincide.

## Links

- Implementation: `crates/steamtron-desktop/ui/style.css`
- Related: `docs/prd/0002-nav-focus.md`
