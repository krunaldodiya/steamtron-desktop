# Full hero reveal and smooth focus scrolling

- Status: done
- Date: 2026-11-15
- Areas: `crates/steamtron-desktop/ui/app.js`

## Problem

On Home, moving focus up from a rail to the hero focused only the Play button.
`scrollControlIntoView` scrolled just enough to reveal that button, so the hero
was left half off-screen. Moving up again to the sticky top menu issued another
small page scroll instead of revealing the whole hero. Repeated native smooth
`scrollBy` calls could also stack while arrow keys were held, producing visible
stutter.

## Goal

Focusing the hero scrolls the entire hero into view, and focusing the Home top
menu keeps it fully visible. Page scrolling is one smooth animation at a time:
a new target replaces the old animation instead of stacking native scroll
requests.

## Non-goals

- Changing rail horizontal scrolling.
- Changing focus order on Library, Store, or game detail pages.
- Replacing scrolling inside Settings panes.
- Adding a reduced-motion preference in this change.

## Requirements

- R1: Focusing `#home-play` or `#home-view` scrolls the page so the whole
  `.home-hero` is visible below the sticky topbar.
- R2: Moving up from the hero to the Home top menu keeps the hero fully visible.
- R3: Moving down from the top menu to the hero uses the same full reveal.
- R4: Page vertical focus scrolling uses one cancellable animation; a new focus
  target cancels and replaces the previous animation.
- R5: The animation is cancelled by wheel or touch input so it does not fight
  the user.
- R6: Focusing sticky topbar controls does not issue a page scroll, except Home
  where it reveals the hero.

## Acceptance criteria

- [ ] Up from the first Home rail lands on Play with the hero fully visible.
- [ ] Up from Play to the Home menu leaves the hero fully visible.
- [ ] Down from the Home menu to Play shows the full hero.
- [ ] Holding arrow keys does not stack or restart native smooth-scroll
  animations.
- [ ] Wheel/touch input cancels an in-flight focus scroll.
- [ ] Library/Store topbar focus does not jump the page.

## Verification

- JSDOM scroll harness passed: focusing Play animates the page to the hero top;
  Up from a card returns to the hero; focusing `#nav-home` keeps it there.
- Existing navigation suites pass unchanged.
- `node --check crates/steamtron-desktop/ui/app.js` passed.
- The full app was rebuilt and installed; the installed binary hash is recorded
  in the handoff.

## Decisions and risks

- A requestAnimationFrame animator replaces native `window.scrollBy` for page
  scrolling. It sets `scrollingElement.scrollTop` each frame and keeps one
  animation owner, so focus changes interpolate from the current position rather
  than queueing.
- Hero reveal targets scroll position 0 because the hero is the first document
  block below the sticky topbar.
- Duration is clamped to 240–520 ms with an ease-out curve; long jumps do not
  feel slow, short moves still animate.
- The custom animator is not compositor-driven like native smooth scroll. The
  single-owner design avoids the larger stutter caused by stacked native calls.

## Links

- Related PRD: `docs/prd/0005-route-focus.md`
- Implementation: `crates/steamtron-desktop/ui/app.js`
