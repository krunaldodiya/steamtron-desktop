# Visible sign-in progress

- Status: done
- Date: 2026-11-16
- Areas: `crates/steamtron-desktop/src/main.rs`, `crates/steamtron-desktop/ui/app.js`

## Problem

Clicking **Sign in** opens the system browser, but the desktop dialog keeps
showing the normal button until the blocking OAuth callback completes. The
command ran on the UI thread, so the webview could not paint the existing
spinner or status update.

## Goal

The sign-in dialog immediately shows that authentication is in progress and
restores the normal controls when it succeeds, fails, or is cancelled.

## Requirements

- R1: Run the blocking browser/OAuth wait off the Tauri UI thread.
- R2: Disable the button and change its label to `Signing in…`.
- R3: Show the existing spinner and an actionable browser-wait status.
- R4: Set `aria-busy` while running and restore the button afterward.
- R5: Preserve the successful `hideAuthGate()` + `load()` flow.

## Acceptance criteria

- [x] The spinner and `Signing in…` appear before the browser callback completes.
- [x] The button cannot be clicked twice while sign-in is running.
- [x] Success hides the gate and loads the app.
- [x] Failure restores the button and shows the error.
- [x] Existing OAuth/PKCE logic is unchanged.

## Verification

- `cargo test -p steamtron-desktop` passed.
- `node --check crates/steamtron-desktop/ui/app.js` passed.
- Release build rebuilt and installed.

## Decisions and risks

- The auth crate remains blocking because it owns the loopback callback and
  browser wait; Tauri wraps it in `spawn_blocking`.
- No timeout/cancel control was added to the dialog.

## Links

- OAuth client: `crates/steamtron-auth/src/lib.rs`
- Sign-in gate: `crates/steamtron-desktop/ui/index.html`
- Tauri command: `crates/steamtron-desktop/src/main.rs`
