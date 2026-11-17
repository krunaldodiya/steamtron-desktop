# Minimal Doctor and Windows backend install

- Status: done
- Date: 2026-11-17
- Areas: `crates/steamtron-doctor`, `crates/steamtron-register`,
  `crates/steamtron-desktop/ui`

## Problem

Doctor exposed process, injection, Lua-directory, managed-ownership and
appmanifest checks that the user does not need. The Windows backend row also had
no working install path. Linux repair must install only SLSsteam Moon, without
Lumen or LuaTools.

## Goal

Doctor has two checks only:

1. Is Steam installed?
2. Is the platform backend installed?

The backend install button is offered only when Steam exists and the backend is
missing. Linux installs only SLSsteam Moon. Windows installs only the
OpenSteamTool/BetterSteamTools runtime DLLs.

## Non-goals

- Process, injection, Lua-directory, appmanifest, or managed-ownership checks.
- Installing Lumen or LuaTools.
- Installing Steam.
- Replacing Steam's own repair tools.

## Requirements

- R1: Doctor reports only `steam-root` and `unlock-backend`.
- R2: When Steam is missing, the backend row is read-only and explains that
  Steam must be installed first.
- R3: When Steam exists and the backend is missing, show the install action.
- R4: When both exist, show no action.
- R5: Linux repair downloads the x86_64 SLSsteam Moon release and runs its
  `setup.sh`; it must not install Lumen or LuaTools.
- R6: Windows repair downloads the pinned OpenSteamTool 1.4.8 release, verifies
  its SHA-256, elevates for the Steam-root copy, and installs only
  `OpenSteamTool.dll`, `dwmapi.dll`, and `xinput1_4.dll`.
- R7: No helper binary is bundled in the application.

## Acceptance criteria

- [x] Doctor returns exactly two checks.
- [x] Missing Steam produces no install button.
- [x] Steam present + backend missing produces an install button.
- [x] Both present produces no action.
- [x] Linux install path contains no Lumen/LuaTools install step.
- [x] Windows install path has a pinned archive and checksum.

## Verification

- `cargo test -p steamtron-doctor` — exactly two check ids.
- `cargo check -p steamtron-desktop`
- `node --check crates/steamtron-desktop/ui/app.js`
- `bash -n crates/steamtron-doctor/scripts/linux-repair-slsteam.sh`
- 0.1.19 Linux binary built and installed; SHA-256
  `50edb75537be7dd39301da5671f0cf75ff2b5957aeb2b99a424db23708361eef`.
- Windows release workflow builds the NSIS installer.
- Windows PowerShell runtime was not executable on this Linux host.

## Decisions and risks

- The Windows backend is implemented with the OpenSteamTool runtime DLLs from
  release 1.4.8 because they match the existing backend file contract.
- The Windows script requires UAC elevation to write the Steam root.
- The Windows PowerShell script was not executed on this Linux host; the
  Windows release build verifies compilation, and runtime needs a Windows VM.

## Links

- `docs/prd/0088-backend-install-without-steam.md`
- `docs/prd/0089-repair-marker.md`
- `crates/steamtron-doctor/scripts/linux-repair-slsteam.sh`
- `crates/steamtron-doctor/scripts/windows-install-bettersteamtools.ps1`
