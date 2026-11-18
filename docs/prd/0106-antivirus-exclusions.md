# Antivirus exclusions cover the unlock backend

- Status: done
- Date: 2026-11-18
- Areas: `crates/steamtron-doctor/scripts/windows-install-bettersteamtools.ps1`

## Problem

SteamTron already offers scoped Microsoft Defender exclusions for game fixes:
`security::planned_paths` lists the fixes pack, overlay upper, mounted overlay,
and optionally the game folder, and `security::require_exception` blocks applying
fixes while those paths are missing.

The BetterSteamTools install had no equivalent. It copies
`OpenSteamTool.dll`, `dwmapi.dll`, `xinput1_4.dll`, and creates
`opensteamtool/` plus `config/stplug-in/` under the Steam root. Defender quarantined `OpenSteamTool.dll` during a real Doctor install; the user
had to restore it from Protection History and allow it manually.

## Goal

Installing BetterSteamTools adds narrowly scoped Defender exclusions for the
staging folder and installed files before any download or copy.

## Requirements

- R1: Add exclusions before downloading or copying anything.
- R2: Exclude the fixed staging folder, the three target DLLs, and the
  `opensteamtool` / `config/stplug-in` directories.
- R3: Never fail the install when Defender is unavailable or tamper protection
  rejects an exclusion; log a warning instead.
- R4: Do not disable real-time protection or add broad drives/Steam root.
- R5: Keep the existing per-game fixes exclusions unchanged.

## Acceptance criteria

- [x] The installer logs one exclusion attempt per path.
- [x] A failed/skipped exclusion still exits with the install result.
- [x] A Doctor reinstall completed without Defender quarantining the backend.

## Verification

- PowerShell parser accepts the updated script.
- `Add-MpPreference` runs before `Invoke-WebRequest`; the copy step checks each
  DLL exists and is non-empty, failing with an antivirus-specific message if it
  disappeared.
- User reinstalled BetterSteamTools through Doctor on 0.1.29 after uninstalling
  it; the install completed without antivirus blocking. The three DLLs are
  present at the Steam root with the release timestamps (OpenSteamTool.dll
  1,742,848 B). `Get-MpPreference` read-back still needs an admin shell; it was
  not used as evidence.

## Decisions and risks

- Exclusions are per-file/per-folder; the whole Steam installation is not
  excluded.
- Defender cmdlets are only present when Microsoft Defender is the active
  antivirus; third-party AV still needs a manual exclusion.

## Links

- Related: `0104-bettersteamtools-1-0-9`, `0105-windows-stplug-registration`.
