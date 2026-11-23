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
- R11: A curated **per-appid catalog** supplies game-specific launch metadata:
  `launch_steam`/`launch_exe`, target executable, prerequisites, AV exclusion,
  known errors/instructions, fix/module hints, and a “needs game-specific exe”
  flag. The catalog stores metadata only; it contains no external fix links.
- R12: The existing **Import fixes / Export fixes / Remove fixes** flow is
  preserved. Built-in modules are additive, tracked by their own manifests, and
  auto-apply never deletes or overwrites user-imported fixes without an explicit
  conflict confirmation. Export includes module files; removing a module removes
  only module-owned files.

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
- [ ] The per-appid catalog supplies the correct target/prerequisites for Far
      Cry 5, Watch Dogs 2, GTA V Enhanced and Max Payne 3.
- [ ] Import/Export/Remove fixes continue to work; auto-apply does not delete
      imported fixes; module removal removes only module-owned files.

## Verification

- Max Payne 3 real-path overlay: desktop Play verified in 0.1.57 (PRD 0135).
- Tool research: Goldberg SocialClub Emu, Uplay R1/R2 unlockers (acidicoala /
  Mr_Goldberg), Anadius Origin Emulator/Unwrapper, EOS emulators.
- Catalog research: Project Lightning `bypass.json` (C# viewer) is keyed by
  Steam appid and records launch mode, target, prerequisites, exclusions,
  instructions and a fix name. Examples: Far Cry 5 552520, Watch Dogs 2 447040
  (`bin/WatchDogs2.exe`), GTA V Enhanced 3240220 (`PlayGTAV.exe`), Max Payne 3
  204100. It is a downloader/catalog, not a generic launcher engine; only the
  metadata idea is used.
- 0.1.58: Far Cry 5 real-path auto-prepare verified; the fixed launcher then
  required administrator elevation (PRD 0138) and exited without starting the
  game. The fixes pack has a Uplay loader but no Steam emulator.
- 0.1.59 code: Play auto-applies GoldBerg when the fixes folder contains a
  launcher module, the game has a Steam API DLL, the fixes do not already ship
  a Steam API DLL, and GoldBerg is not applied. This is a first slice of R1/R3,
  not the full registry.
- 0.1.59 runtime: Far Cry 5 launched through the real path with the imported
  CODEX/Uplay fix plus auto-applied GoldBerg, with SteamTron elevated (game PID
  6428). The imported Uplay loader and game-specific exe are still required;
  built-in Uplay replacement is not implemented.
- Not run yet: GTA V.
- Not implemented yet: Uplay/SocialClub/EA modules, full analyzer, module UI,
  per-appid catalog, real-path prepare UI.

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
