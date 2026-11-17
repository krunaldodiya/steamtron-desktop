# Linux SLSsteam-only repair

- Status: done
- Date: 2026-11-17
- Areas: `crates/steamtron-doctor`, `crates/steamtron-register`,
  `crates/steamtron-library`, `crates/steamtron-desktop`

## Problem

SteamTron's Linux Doctor can check SLSsteam Moon and can hand off to an
installer, but the repair path was built for the old 32-bit Steam client:

- It requires a local `~/Downloads/SLSteam-moon/install-slsteam-moon.sh`.
- It does not download the current x86_64 SLSsteam Moon build.
- It strips the Lumen step but still depends on the LuaTools installer layout.
- It accepts a 32-bit `SLSsteam.so` as “ready” because readiness only checks
  that the file exists.
- It checks `LD_AUDIT` for an `SLSsteam` string without checking whether the
  loaded library is 64-bit.

With the current 64-bit Steam client, Steam logs:

```text
wrong ELF class: ELFCLASS32; ignored
```

Doctor can therefore report the backend as ready while game registration is
not active. The repair also only recreates appmanifests; it does not re-register
games that were added to the SteamTron library while Steam registration was
unavailable.

## Goal

Linux Doctor checks the architecture of the installed SLSsteam Moon hook, and
its repair installs only the x86_64 SLSsteam Moon runtime into the user's global
XDG data directory. No SLSsteam, Lumen or LuaTools files are bundled in the
SteamTron application or copied from a local checkout.

After a successful repair, games already in the SteamTron library are also
registered in Steam when Steam is installed.

## Non-goals

- Installing or requiring Lumen.
- Installing or requiring the LuaTools plugin.
- Supporting the legacy 32-bit Steam client with this repair path.
- Installing Steam itself.
- Installing BetterSteamTools on Windows; Windows repair remains a separate
  implementation.

## Requirements

- R1: The Linux repair downloads the latest x86_64 SLSsteam Moon release at
  runtime and installs it under `~/.local/share/SLSsteam` using the release's
  own `setup.sh`.
- R2: No SLSsteam, Lumen or LuaTools folder is copied into the application,
  bundled as a Tauri resource, or committed as a binary asset. The only
  added application payload is the repair script text.
- R3: If an x86_64 SLSsteam Moon hook and wrapper are already present, repair
  does not download the release again.
- R4: A 32-bit `SLSsteam.so` is reported as incompatible with the 64-bit Steam
  client and is not treated as ready by the registration backend.
- R5: Repair removes an old SLSsteam installation, restores the stock system
  Steam launcher if needed, installs the x86_64 runtime, and verifies both
  `SLSsteam.so` and `library-inject.so` are ELF64.
- R6: Repair preserves Steam, installed games, and `config/stplug-in` scripts.
- R7: Repair does not install Lumen or the LuaTools plugin.
- R8: After the backend is installed, repair re-registers managed games that
  are missing a Steam registration, including games added to SteamTron while
  the backend was unavailable.
- R9: Repair remains user-consented and opens in a terminal with a log.

## Acceptance criteria

- [x] Doctor marks a 32-bit SLSsteam backend as not ready.
- [x] Repair no longer reads `~/Downloads/SLSteam-moon/install-slsteam-moon.sh`.
- [x] Repair skips the download when an x86_64 hook and wrapper are already
  installed (verified against the current install).
- [x] Repair installs only SLSsteam Moon; no Lumen/LuaTools install step.
- [x] Existing managed games without `steam.registered` are re-registered after
  repair (unit test).
- [x] Steam, games, and `stplug-in` are not removed by the script.
- [x] The application adds only the 5,135-byte repair script text; no
  SLSsteam/Lumen/LuaTools binary directory is bundled.

## Verification

- `bash -n crates/steamtron-doctor/scripts/linux-repair-slsteam.sh` — pass.
- `bash crates/steamtron-doctor/scripts/linux-repair-slsteam.sh` on the current
  x86_64 install — printed “already installed … No download was needed”, exit 0.
- `cargo test -p steamtron-register -p steamtron-doctor -p steamtron-library`:
  - register: 4 passed, including `marks_a_32_bit_hook_as_not_ready` and
    `identifies_elf_class_from_the_header`
  - doctor: 2 passed
  - library: 30 passed, including
    `repair_registers_a_game_added_without_a_backend`
- `cargo check -p steamtron-desktop` — pass.
- `npx @tauri-apps/cli@2 build --no-bundle` — release binary built.
- Installed to `~/.local/bin/steamtron-desktop`; SHA-256 matched the build:
  `f045a09588ac63c141666798e0ec567d5247b4c23ff85f3b21825266c02a3b5c`.
- Only `crates/steamtron-doctor/scripts/linux-repair-slsteam.sh` (5,135 bytes)
  was added; no binary or helper folder was added.
- The full download-and-install branch was not run against the user's live
  Steam installation. Release URL resolution, SHA-256 download, archive
  extraction, and `setup.sh` presence were verified from the current release;
  the installer itself was not re-run because it stops Steam and requires sudo.

## Decisions and risks

- SLSsteam Moon is the only Linux backend dependency. Lumen and LuaTools are
  intentionally excluded.
- The repair script is embedded as text with `include_str!`; it is not a Tauri
  resource folder and adds only a few KiB to the binary.
- The script downloads third-party release assets from the x86_64 continuation
  repository at repair time. It does not redistribute them.
- Repair still requires `sudo` for the system Steam launcher and may prompt in
  the terminal.
- The script intentionally preserves `config/stplug-in`; removing game Lua
  registrations is a separate destructive action.

## Links

- `docs/PRD.md` Linux unlock section
- `crates/steamtron-doctor/src/lib.rs`
- `crates/steamtron-register/src/linux.rs`
- `crates/steamtron-library/src/managed.rs`
