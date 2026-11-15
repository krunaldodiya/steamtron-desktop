# Guest sign-in screen and default-browser login

- Status: done
- Date: 2026-11-15
- Areas: `crates/steamtron-desktop/ui/index.html`, `crates/steamtron-desktop/ui/app.js`, `crates/steamtron-auth/src/lib.rs`

## Problem

On launch without a session the app immediately starts OAuth and opens a
browser. The gate then offers only Retry and a primary Quit button, so the
screen reads as "sign in or exit" rather than a guest sign-in screen. After
logout the same gate appears without a sign-in action.

The Linux browser launcher also hard-codes Firefox first, so OAuth opens
Firefox even when another browser is the desktop default.

## Goal

An unauthenticated launch shows a deliberate sign-in screen with a primary
Sign in action. Nothing opens until the user clicks it. Logout returns to that
screen and leaves the app running. Sign-in uses the system default browser.

## Non-goals

- Guest access to the library or Store.
- Changing the OAuth callback, token storage, or server flow.
- Changing the account dialog layout.
- Removing the explicit Quit action.

## Requirements

- R1: Launch without a session shows the auth gate with an enabled
  **Sign in** button and no automatic `profile_login` call.
- R2: Clicking Sign in starts the existing OAuth flow and opens the URL with
  the system default browser.
- R3: Browser discovery must not prefer or hard-code Firefox, Chromium, or any
  named browser; the OS default opener decides.
- R4: A failed or incomplete sign-in keeps the gate visible, shows the error,
  and re-enables Sign in.
- R5: Logout from the account dialog or Settings closes the dialog, returns to
  the gate with a sign-in action, and does not quit the app.
- R6: Quit remains a separate secondary action that exits only when clicked.
- R7: While the gate is visible the app content stays inert/hidden as today.

## Acceptance criteria

- [x] Fresh unauthenticated launch shows Sign in and never calls
  `profile_login` before it is clicked.
- [x] Clicking Sign in calls `profile_login` and shows the opening-browser
  status.
- [x] A rejected login restores the Sign in button with the error text.
- [x] Logout returns to the gate with Sign in and no `quit_app` call.
- [x] Quit still calls `quit_app` only when clicked.
- [x] No named-browser fallback list remains in the auth crate.

## Verification

- `node --check crates/steamtron-desktop/ui/app.js` passed.
- jsdom auth harness `/tmp/steamtron-nav-test/auth-test.mjs` passed all 13
  checks: idle start screen, no automatic OAuth, sign-in click, spinner and
  disabled state, successful entry, logout without quitting, failed-login
  recovery, and click-only Quit.
- `cargo test -p steamtron-auth` compiled and passed.
- `grep` for `FALLBACK_BROWSERS`, `STEAMTRON_BROWSER`, `gate-retry`, and
  `firefox` is clean across the UI and auth crate.
- Release build completed and the installed binary hash matched the build.

## Decisions and risks

- The gate remains a full-screen backdrop, not a dismissible modal: the
  application needs an account for API-backed screens.
- `webbrowser::open` is used on every platform; on Linux it delegates to the
  desktop default URL handler instead of spawning a named browser.
- The sign-in call still blocks for the OAuth callback listener, so the
  spinner and disabled button remain while it runs.

## Links

- Related: `docs/prd/0002-nav-focus.md`
