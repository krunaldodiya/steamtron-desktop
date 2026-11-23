# Prefer game-bundled redistributables for runtime provisioning

- Status: done
- Date: 2026-11-23
- Areas: `steamtron-desktop` runtime provisioning

## Problem

Playing Watch Dogs 2 after a successful restore failed with:

```text
C:\Users\krunaldodiya\AppData\Local\steamtron\runtimes\vc2012-x64.exe is not a valid Microsoft-signed installer
```

The game already ships the correct Microsoft-signed VC++ 2012 installers:

- `_Redist\vcredist_x64_2012_x64.exe` (7,186,992 bytes, status Valid,
  subject `CN=Microsoft Corporation`)
- `_Redist\vcredist_x86_2012_x86.exe` (6,554,576 bytes, status Valid)

Steam's install script runs those from the game folder; SteamTron ignored them
and downloaded its own copy. A stale/partial cached download then failed the
signature check and blocked Play. The same pattern affected earlier Little
Nightmares runtime installs.

## Goal

Use the Microsoft-signed redistributable shipped with the game first. Fall back
to the verified download, retry once, and treat reboot-required success codes
as success.

## Requirements

- R1: Before downloading, scan the game folder's known redistributable
  directories (`_Redist`, `_CommonRedist`, `Redist`, `Support`, `Installers`,
  `Runtimes`, and direct files) for an installer matching the missing version
  and architecture.
- R2: Verify the bundled installer with the same Microsoft-signature check; skip
  unsigned candidates.
- R3: Only match names containing `vcredist`/`vc_redist` with the correct year
  and `x64`/`x86` token. `vc_redist.x64.exe` without a year is treated as the
  2015-2022 package only.
- R4: Fall back to the existing download, retrying once after removing an
  invalid cached/partial file.
- R5: Accept exit codes 3010 and 1641 (success, restart required) from runtime
  installers.
- R6: No arbitrary installer execution; the allow-list and signature check stay.

## Acceptance criteria

- [x] Unit test covers bundled name/version/architecture matching.
- [x] Watch Dogs 2 Play installs VC++ 2012 from `_Redist` without the signature
      error.
- [ ] A missing runtime with no bundled installer still downloads and verifies.
- [ ] A runtime installer returning 3010 does not fail Play.

## Verification

- Data: both Watch Dogs 2 `_Redist` installers are valid Microsoft-signed.
- Code: `cargo check -p steamtron-desktop --bin steamtron-desktop` passes.
- Code: unit test added for `bundled_name_matches`.
- Installed build: 0.1.52 on Windows 11, Watch Dogs 2 Play on 2026-11-23.
  `diagnostics/21c25088-94fd-4581-b059-0d071aa66585/play-1795414424.log`:
  ```text
  [runtime] missing Visual C++ 2012 Redistributable (x64) for MSVCR110.dll
  [runtime] using bundled D:\SteamTronLibrary\steamtronapps\common\Watch Dogs 2\_Redist\vcredist_x64_2012_x64.exe for Visual C++ 2012 Redistributable (x64)
  [runtime] waiting for the Windows elevation prompt
  [runtime] installed Visual C++ 2012 Redistributable (x64)
  SteamTron overlay: merged=S:\
  SteamTron overlay: exec=S:\bin\WatchDogs2.exe
  ```
  The user confirmed the game started and ran normally.

## Decisions and risks

- Bundled installers are preferred only when the filename identifies the
  exact VC++ generation and architecture, and the Authenticode signature names
  Microsoft. This avoids both wrong-version installs and untrusted repack
  binaries.
- Direct folder scan depth is bounded; known redistributable directories are
  checked first to avoid walking large game data trees.
- The remote URL still exists and returns the correct file now; the retry and
  bundled preference make Play resilient to transient/stale downloads.

## Links

- [0098](0098-windows-game-runtimes.md) — runtime provisioning.
- [0122](0122-playstation-runtime-provisioning.md) — bundled runtime installs.
