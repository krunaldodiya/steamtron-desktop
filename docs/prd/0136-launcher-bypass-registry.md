# Launcher-bypass module registry and one-click auto-fix

- Status: active
- Date: 2026-11-23
- Areas: `steamtron-desktop` fixes/tools, `steamtron-core` detection, Windows real-path overlay

## Problem

Steamless covers SteamStub and Goldberg covers Steamworks, but launcher-protected
games still need per-game LuaTools fix zips. The zips bundle generic launcher
emulator DLLs plus game-specific exe/config patches. The product should apply
the generic launcher emulators itself, auto-detect what a game needs, and let
the user click one Apply button.

The current Max Payne 3 path already proves the launch mechanism: real-path
WinFsp mount + local junction + fixes overlay, with the launcher's own directory
as cwd.

## Goal

Maximize launcher-bypass coverage through built-in, hash-pinned modules; detect
the required layers per game; apply them into the fixes overlay; prepare the
real-path launch; and report what is unsupported instead of silently doing
nothing.

## Requirements

- R1: A registry of launcher/DRM modules with detection, apply, remove and
  status (SteamStub, Steam API, Uplay R1/R2, SocialClub, RGL, EA/Origin, EOS,
  Battle.net, Bethesda, launcher-skip helpers).
- R2: Detection from PE sections/imports/strings, game files, installscript,
  launcher executables and known redirector names.
- R3: “Apply recommended” runs the detected modules in conflict-safe order and
  can be overridden per module.
- R4: Assets are downloaded once to an app cache, hash-verified and reused; no
  per-game LuaTools download for generic layers.
- R5: Steam emulator conflicts are detected (Goldberg vs CreamAPI/other
  `steam_api.dll`); launcher modules are selected, not stacked blindly.
- R6: Launch target preference: fix-provided/unprotected launcher first, then
  real game exe, never a launcher redirector when the real target exists.
- R7: Apply `installscript.vdf` registry values against the real game path.
- R8: Prepare the real-path overlay layout (base, mountpoint, junction,
  sidecar) from the UI, with remove/restore.
- R9: UI status per layer: applied, skipped, not needed, needs game-specific
  exe patch, unsupported.
- R10: Never modify the installed game files; everything lands in the fixes
  overlay/base layout.

## Coverage target

- SteamStub → Steamless.
- Steamworks → Goldberg/GSE.
- Ubisoft → Uplay R1/R2 loader/unlocker.
- Rockstar SocialClub → Goldberg SocialClub Emu.
- Rockstar RGL `.rkstr` → game-specific unprotected exe/RGL shim; flag when no
  built-in patch exists.
- EA/Origin → Anadius Origin Emulator + Origin Unwrapper/Helper.
- Epic Online Services → EOS emulator where available.
- Battle.net / Bethesda.net → detection and clear unsupported status unless a
  reliable emulator is added.
- Launcher-skip helpers (2K/Paradox and similar) where the launcher is optional.
- Denuvo, online/anti-cheat and Microsoft Store DRM are out of scope.

## Acceptance criteria

- [ ] Far Cry 5 can be made playable with built-in Uplay + Steamless/Goldberg +
      real-path overlay, without a LuaTools fix zip (pending game restore).
- [ ] GTA V/Max Payne 3 detection reports SocialClub and RGL; SocialClub is
      applied; an RGL-specific exe requirement is reported if present.
- [ ] Max Payne 3 keeps working through the existing real-path setup.
- [ ] The UI shows module status and can remove each module.
- [ ] Unsupported launchers are reported, not silently ignored.
- [ ] Real-path layout can be prepared/removed for any game from the UI.

## Verification

- Max Payne 3 real-path overlay: desktop Play verified in 0.1.57 (PRD 0135).
- Tool research: Goldberg SocialClub Emu, Uplay R1/R2 unlockers (acidicoala /
  Mr_Goldberg), Anadius Origin Emulator/Unwrapper, EOS emulators.
- Not run yet: Far Cry 5, GTA V.
- Not implemented yet: modules, analyzer, auto-apply UI, real-path prepare UI.

## Decisions and risks

- Generic emulator DLLs can be bundled/hash-pinned; game-specific exe patches
  cannot be synthesized reliably. RGL/EA/Denuvo titles may still need a
  per-game patch; the app must say so.
- The module registry is the extension point; adding a new launcher is a
  detection rule plus an asset module.
- Online/anti-cheat games remain unsupported; local/LAN emulation only.
- Third-party emulator licensing/hash verification must be handled per module.

## Links

- PRD 0135 (real-path WinFsp overlay)
- PRD 0132 (bundled runtimes, done)
- PRD 0131/0134 (installer, done)
