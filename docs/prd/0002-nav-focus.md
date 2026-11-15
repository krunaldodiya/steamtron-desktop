# Restore keyboard and controller focus across Home, Library, and Store

- Status: done
- Date: 2026-11-15
- Areas: `crates/steamtron-desktop/ui/app.js`

## Problem

Three focus regressions broke keyboard and gamepad navigation:

1. Arrow keys and controller input did nothing until Escape was pressed once.
   The account dialog is a permanent `.modal-backdrop.hidden` element. Focus
   lookup took the first `.modal-backdrop` as the navigation root even when it
   was hidden, found no controls inside it, and returned without moving focus.
   Escape then fell through to the generic "remove the first modal" branch and
   deleted the account dialog, which made navigation start working.
2. Entering Home, Library, or Store did not focus the first card. With nothing
   focused, the first arrow press picked the first document control (the top
   menu) instead of the first card in the active route. A fresh launch also left
   focus on the window body.
3. Opening a card showed the game or store detail page, but Escape returned to
   the route with focus on the document body. The original card lost its focus
   outline and the next arrow restarted from the header.

## Goal

The active route always has a useful focused card, arrows and the controller
work immediately after launch without an Escape warm-up, and returning from a
detail page puts focus back on the card that opened it.

## Non-goals

- No redesign of the spatial navigation scoring or the header layout.
- No change to gamepad polling, debounce, or repeat timing.
- No new persisted per-route focus storage across restarts.

## Requirements

- R1: Hidden `.modal-backdrop` elements are never treated as the navigation
  root; only a visible modal can own focus.
- R2: Escape removes or closes only a visible modal, and never removes the
  account dialog element from the DOM.
- R3: Entering Home, Library, or Store (navigation click, back navigation, or
  first launch) focuses the first card of that route; the first arrow press and
  Down from the header also fall back to that card rather than the header or the
  Store search box.
- R4: Opening a card remembers its route identity (`data-managed` for Home and
  Library, `data-store-id` for Store). Escape and the in-page Back button
  restore focus to that card, or to the first card when it no longer exists.
- R5: The Store waits for its result cards while loading instead of parking
  focus on the search box, then focuses the first card when results paint.
- R6: A card opened from Home returns to Home (Back label and header highlight)
  rather than the Library.

## Acceptance criteria

- [x] A fresh launch focuses the first Library card, and ArrowRight moves to the
  next card without pressing Escape first.
- [x] Escape on the Home, Library, or Store route still leaves the account
  dialog element in the DOM.
- [x] Clicking a card, opening its detail, and pressing Escape restores the
  focus outline and `document.activeElement` to that same card.
- [x] Clicking Home or Store from the header focuses the first card in that
  route.
- [x] The in-page Back button uses the same restore behavior as Escape.
- [x] A Home card detail shows `← Home` and returns to Home on Escape.
- [x] A Store card detail returns to Store on Escape and restores that card.
- [x] Down from the Store header focuses the first card.

## Verification

- `node --check crates/steamtron-desktop/ui/app.js` — no syntax errors.
- jsdom navigation harness (stubs Tauri `invoke`, provides element geometry,
  loads the real `index.html` and `app.js`) printed:

  ```
  PASS boot focuses the first library card
  PASS ArrowRight moves to the next card before any Escape
  PASS Escape from a detail restores the clicked card
  PASS Home entry focuses the first home card
  PASS Home card detail labels Back as Home
  PASS Escape returns a Home detail to its card
  PASS Store entry focuses the first store card
  PASS Down from the Store header focuses the first card
  PASS Escape returns a Store detail to its card
  PASS Escape keeps the account dialog element in the DOM
  ```

- The same harness against `HEAD` before this change timed out waiting for the
  first library card focus, reproducing the reported regression.

## Decisions and risks

- Focus memory is in-memory only. It is intentionally dropped on reload; the
  first card is the safe fallback.
- The Store entry focus is deferred through `pendingRouteFocus` while the
  catalogue paints, so a fast nav click during warm-up still lands on a card.
- The old document-level Escape handler removed every `.modal-backdrop` from the
  DOM. It was removed; `dispatchNavigation` now handles visible modals, including
  the case where the focused element is a text field inside a modal.

## Links

- Implementation: `crates/steamtron-desktop/ui/app.js`
- Related queue item: `docs/prd/0003-startup-steam-flash.md`
