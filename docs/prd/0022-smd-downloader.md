# Self-provisioned SMD downloader

- Status: done
- Date: 2026-11-15
- Areas: `crates/steamtron-desktop/src/download.rs`, `crates/steamtron-desktop/src/main.rs`

## Problem

Download and Verify required an `smd` binary from UmuTron or `PATH`; otherwise
the UI showed "smd was not found. Install UmuTron or set STEAMTRON_SMD."
The application did not provide or prepare the downloader itself, and the old
configuration forced DepotDownloaderMod (a .NET path) instead of SMD's native
Rust engine.

## Goal

SteamTron provides the downloader automatically and uses the native, resumable
engine. Users never install UmuTron just to download or verify a game.

## Non-goals

- Reimplementing Steam's depot protocol inside SteamTron.
- Bundling the 94 MB binary in the repository or installer.
- Changing the Lua/manifest provider.

## Requirements

- R1: `resolve_smd` first honors `STEAMTRON_SMD`, the SteamTron tools directory,
  then `PATH`.
- R2: When absent, SteamTron downloads the pinned SMD 1.5.0 terminal release
  from its GitHub release, verifies SHA-256, marks it executable, and caches it
  under the SteamTron tools directory.
- R3: Linux and Windows use their pinned release asset and checksum.
- R4: Provider and hashing work happens off the UI thread; the downloader is
  prepared in a background thread at startup and again on first use if needed.
- R5: SMD is configured to use its native Rust engine (no .NET runtime).
- R6: Existing CLI arguments and JSON progress events remain compatible:
  `download <lua> -o <root> --like-steam --platform windows --language english --json`
  and `download <lua> --update <root> --repair ...`.

## Acceptance criteria

- [x] A missing SMD no longer produces an "install UmuTron" error.
- [x] The pinned binary is checksum-verified before use.
- [x] Provisioning/verification does not block the UI thread.
- [x] Native engine is selected instead of DepotDownloaderMod.
- [x] Start/Verify commands keep their existing arguments and result handling.

## Verification

- `cargo check -p steamtron-desktop` passed.
- Downloaded both pinned release assets directly and verified their SHA-256:
  - Linux `Steam-Manifest-Downloader-Terminal_1.5.0_linux-x64`
    (`e8819e62...3021c2`, 93,931,664 bytes).
  - Windows `..._windows-x64.exe` (`7cb56e33...4cfa138`, 24,039,752 bytes).
- `smd download --help` confirms the flags used by `download.rs`;
  `smd download <lua> --list --json` produced the expected depot JSON.
- SMD JSON events (`started`, `output` with completed/total bytes,
  `depot_complete`, `complete`, `error`) match the parser's handling.
- Runtime provisioning test with an empty `STEAMTRON_DATA_HOME`: the app
  created `<data>/steamtron/tools/smd` (93,931,664 bytes, mode 0755) with the
  pinned SHA-256 `e8819e62...3021c2` without user interaction.
- Installed binary SHA-256:
  `62b95ec5a19948d8536d8d5b5ee86f635549be7ebbcc36ecd41094c802cd5c86`.

## Decisions and risks

- Runtime provisioning keeps the repository and installer small while removing
  the external installation requirement.
- The release version and hashes are pinned. Updating SMD is a code change with
  a new checksum.
- A network failure while preparing SMD still surfaces a clear error on the
  action; the startup prewarm retries on the next launch.

## Links

- SMD: https://github.com/MCbabel/Steam-Manifest-Downloader
- Related: `docs/prd/0018-steamless-operations.md`
