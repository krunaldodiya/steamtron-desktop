# Proton and prefix defaults without Steam

- Status: done
- Date: 2026-11-15
- Areas: `crates/steamtron-desktop/src/proton_sources.rs`, `crates/steamtron-desktop/src/main.rs`, `crates/steamtron-core/src/library.rs`

## Problem

The Proton Manager's Valve-Proton tab failed with "Steam installation was not
found" even though SteamCMD can download Valve Proton without Steam. Play
reported "Proton not found" when the configured runner path was missing.
Prefix source had no global default.

## Goal

Valve Proton channels are listed and installable without a Steam library. New
games use a configurable global prefix source. Play uses an installed Proton
fallback instead of failing when the configured path is missing.

## Non-goals

- Automatically downloading the exact missing Proton from Play.
- Changing per-game Compatibility options.

## Requirements

- R1: Valve release discovery treats a missing Steam library as "no previously
  installed runners" and still returns downloadable channels.
- R2: General Settings exposes a default prefix source (umu or steam) stored in
  `library.json`.
- R3: New managed games use the global default prefix mode.
- R4: Play checks the configured Proton path; if missing, it selects any
  installed runner under the SteamTron runners directory.
- R5: Play reports a clear "no Proton installed" error only when no runner is
  available.

## Acceptance criteria

- [x] Valve Proton lists without a Steam library.
- [x] Default prefix source persists and applies to new games.
- [x] Play falls back to an installed Proton runner.
- [x] Proton Manager still shows installed runners.

## Verification

- `cargo check -p steamtron-desktop` passed.
- `cargo test -p steamtron-library --lib`: 23/23.
- Dialog UI suite 73/73.
- Valve `proton_sources::valve_releases` no longer propagates a Steam library
  error; `play_managed` resolves an installed runner before planning.

## Decisions and risks

- The global prefix default is `umu` for new installs; existing games keep
  their per-game mode. Users can choose Steam compatdata globally.
- The fallback does not modify the saved game; it only selects a runnable
  Proton for the current launch.

## Links

- Related: `docs/prd/0018-steamless-operations.md`
