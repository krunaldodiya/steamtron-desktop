# Launch on the Home screen

- Status: done
- Date: 2026-11-15
- Areas: `crates/steamtron-desktop/ui/app.js`, `crates/steamtron-desktop/ui/index.html`

## Problem

SteamTron always opened on the Library route on launch. The user expects the
Home screen (recently played and ready-to-play rails) to be the landing view.

## Goal

A fresh launch paints Home, shows Home as the active top-nav item, and focuses
its first card so keyboard/controller navigation starts there.

## Non-goals

- Remembering the last route across restarts.
- Changing the order of top-nav buttons.

## Requirements

- R1: The initial `appView` is `home`.
- R2: The initial header highlight and `aria-current` are on Home.
- R3: The existing route-focus logic focuses the first Home card on launch.
- R4: If no managed library exists, the legacy Steam library can still fall
  back to its Library view instead of showing an empty Home.

## Acceptance criteria

- [x] Launching with a managed library paints the Home rails and hero.
- [x] The top bar shows Home as the active item on launch.
- [x] The first Home card receives focus on launch.
- [x] With no managed library, the legacy shell still renders its games list.

## Verification

- jsdom harness: `boot focuses the first home card` passes after launch.
- `node --check` passes for `app.js`.
- Built application launched and inspected: Home is the first screen.

## Links

- Related: `docs/prd/0002-nav-focus.md`
