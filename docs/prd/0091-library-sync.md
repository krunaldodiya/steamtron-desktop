# Sync SteamTron library to Steam

- Status: done
- Date: 2026-11-17
- Areas: `crates/steamtron-desktop`, `crates/steamtron-library`

## Problem

A game can be added while Steam or the unlock backend is unavailable. SteamTron
keeps it in its own library, but Steam never receives the Lua registration. If
Steam and SLSsteam Moon/BetterSteamTools are installed later, future additions
go to both libraries but the earlier games remain SteamTron-only.

## Goal

Doctor offers a third action, Sync Steam library, which registers every managed
SteamTron game that is not already registered with Steam. It is enabled only
when the Steam installation and platform backend checks are both healthy.

## Non-goals

- Downloading game content.
- Installing Steam or the backend.
- Removing games from either library.
- Syncing games that have no Steam App ID or persistent manifest bundle.

## Requirements

- R1: Sync is disabled unless both Doctor checks are healthy.
- R2: Sync calls the existing managed-game registration repair path.
- R3: Already-registered games are left untouched.
- R4: Each missing game is registered using its persisted Lua/manifest bundle.
- R5: Missing appmanifests are recreated after registration.
- R6: Failures are reported per App ID; successful games remain synced.
- R7: No content is downloaded and no Steam restart happens automatically.

## Acceptance criteria

- [x] Sync registers a game added while no backend was available (unit test).
- [x] Sync reports already-registered games separately.
- [x] Sync is disabled when either Doctor check fails.
- [x] Sync is available when both checks pass.
- [x] No game content is downloaded.

## Verification

- Existing `repair_registers_a_game_added_without_a_backend` test covers the
  registration path.
- `cargo check -p steamtron-desktop`
- `node --check crates/steamtron-desktop/ui/app.js`
- 0.1.21 release workflow succeeded for Linux and Windows from commit
  `2ce5018`.
- Windows installer SHA-256:
  `0476005af25262f11135e4ff62065bb94969c7bca748ce045b666404c82dbd4d`.
- Linux assets were published with matching `SHA256SUMS.txt`.

## Decisions and risks

- Sync reuses the persistent bundle store, so old games do not require the
  manifest provider to be online.
- A game whose bundle is missing is reported and must be re-added.
- Steam must be restarted manually after syncing.

## Links

- `crates/steamtron-library/src/managed.rs`
- `crates/steamtron-desktop/src/main.rs`
- `docs/prd/0090-minimal-doctor.md`
