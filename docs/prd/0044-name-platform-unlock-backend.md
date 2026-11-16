# Name the platform unlock backend

- Status: done
- Date: 2026-11-16
- Areas: `crates/steamtron-doctor/src/lib.rs`,
  `crates/steamtron-desktop/ui/app.js`

## Problem

Doctor showed “Steam registration backend”, which was easy to confuse with the
“Steam installation” check. The backend is a specific tool: SLSsteam Moon on
Linux and BetterSteamTools on Windows.

## Requirements

- R1: Linux Doctor check title is `SLSsteam Moon`.
- R2: Windows Doctor check title is `BetterSteamTools`.
- R3: The managed-ownership error names the active platform backend.
- R4: The repair button names the backend rather than saying “Install backend”.
- R5: Generic UI copy says “unlock backend”, not “registration backend”.

## Verification

- Ran `steamtron_doctor::run()` on Linux:
  - `steam-root | Steam installation | Error | Steam installation was not found`
  - `unlock-backend | SLSsteam Moon | Error | Steam was not found`
- `cargo check -p steamtron-doctor -p steamtron-desktop` passes.
- `node --check crates/steamtron-desktop/ui/app.js` passes.

## Decisions and risks

- The Windows title is compile-time selected with `cfg(target_os = "windows")`;
  it was not executable on this Linux host.
