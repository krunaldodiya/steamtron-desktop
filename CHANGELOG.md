## [0.1.44] - 2026-11-22

### Fixed

- Restore now selects the game's `appmanifest_*.json` instead of the
  `appmanifest_*.cache.json` sidecar. NTFS lists the cache file first, so a
  complete format-2 backup such as The Witcher 3 failed with
  "Unsupported SteamTron backup format".
- Fixes are optional during restore: a backup without a fixes archive no longer
  removes an existing fixes folder, and a stale `fixes_directory` without an
  archive is ignored.

## [0.1.43] - 2026-11-22

### Fixed

- Restoring a backup for a game that is not in the library no longer downloads
  the manifest bundle from the API. Restore rebuilds the entry from the
  backup's own appmanifest, Lua and depot manifests through the metadata import
  path, so a complete backup restores without network access. The Add game flow
  keeps its provider download.
- Fixes archives are restored whenever the backup declares a fixes component,
  even if the legacy `fixes_directory` field is absent. The backup's
  `fixes_enabled` state is applied before the commit, and fixes are committed
  after the game content.

## [0.1.42] - 2026-11-22

### Fixed

- Microsoft Defender no longer quarantines a game's fixes when they are
  imported, restored or launched. SteamTron now adds the scoped exclusions
  automatically, including process exclusions for the overlay wrapper and the
  game executables (the only form that matches files opened through the WinFsp
  overlay drive), and resolves the managed fixes folder instead of the legacy
  app-data path. One elevation prompt per game; later launches are silent, and
  the Security panel reflects the recorded exclusions.

## [0.1.41] - 2026-11-22

### Added

- PlayStation PC ports that ship `install_pspc_sdk_runtime.bat` with the
  PlayStation PC SDK runtime MSIs in the game folder now install that runtime
  before the first launch, the way Steam's install script does. The check is
  skipped when the runtime is already installed; a declined or failed install
  is written to the play log and never blocks the game.

### Fixed

- Ghost of Tsushima no longer reports "PlayStation(R) PC SDK Runtime is
  missing" from its `crs-video.exe` helper after the game was restored or
  downloaded without Steam's install script running.

## [0.1.40] - 2026-11-21

### Fixed

- Restoring a game backup no longer fails on Windows with
  `The directory name is invalid. (os error 267)` when the backup's fixes or
  manifest TAR used the raw game title as its folder (for example
  `Marvel's Spider-Man: Miles Morales`). Restore maps that folder to the
  installed folder recorded in the appmanifest, skips directory entries it
  does not need, and names the offending entry when a file name cannot be
  created on the current platform.
- Restore checks the fixes and manifest-bundle TARs before copying the game
  content, so a backup that cannot be restored fails in seconds instead of
  after a multi-gigabyte copy.

### Changed

- Whole-game backups name the fixes component with the same `<game>.tar` name
  as the standalone fixes export, and the legacy fixes export uses the fixes
  folder's name for the TAR wrapper instead of the raw title. Metadata repair
  also writes a sanitized manifest-bundle wrapper.

## [0.1.39] - 2026-11-19

### Changed

- The Game Link and Fixes tabs now use one "Add launcher…" picker. The dialog
  can browse anywhere and accepts a launcher from the game folder or its fixes
  folder; the separate fixes-only picker was removed.

### Fixed

- Play stays on Stop while a game's launcher has exited but the game it started
  is still running. Managed Windows launches now track the whole process tree,
  and Stop terminates it, so games such as The Witcher 3 no longer flip back to
  Play right after starting.

## [0.1.38] - 2026-11-19

### Added

- Launchers that ship inside a game's fixes folder (for example RDR2's
  `Launcher.exe`) can be added from the Fixes tab or the Game Link tab and run
  from the merged overlay view instead of the raw fixes folder.

### Fixed

- The fixes overlay stays mounted until every process the launcher started has
  exited, instead of tearing down the merged drive when the first launcher
  process ends.
- Launch-target checks accept Windows paths that differ in case or carry the
  `\\?\` prefix, and the merged-drive rewrite covers fixes-folder paths.

## [0.1.37] - 2026-11-19

### Fixed

- Fixes for a linked game whose prefix or manifest lives in SteamTronLibrary now
  import to `<drive>\SteamTronLibrary\steamtronapps\fixes\<game>` instead of
  `%LOCALAPPDATA%\steamtron\fixes\<appid>`; existing app-data fixes move there
  automatically.

## [0.1.36] - 2026-11-19

### Changed

- Backups name the fixes component `<game>.tar` instead of `fixes.tar`; restore reads the manifest name and still accepts old backups.
- Fixes import and export show an inline progress bar with percent and cancellation.

## [0.1.35] - 2026-11-19

### Changed

- Fixes import and export use TAR only; the ZIP reader/writer and the backup-update action were removed.

## [0.1.34] - 2026-11-19

### Changed

- Fixes export now writes a backup-compatible `fixes.tar`
  (`fixes/<game folder>/...`); import accepts TAR only.

### Fixed

- Linux Play now passes the game's launch environment through the fixes overlay
  helper. Without it, Proton ignored `WINEDLLOVERRIDES=steam_api64=n,b` and
  `PROTON_DISABLE_LSTEAMCLIENT=1`, so GoldBerg packs exited after the splash
  even though the same fixes worked on Windows.
- TAR export no longer overflows the 1 MiB Windows main-thread stack; the
  SHA-256 buffer is heap allocated.

# Changelog

All notable changes to SteamTron Desktop are recorded here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and releases use [Semantic Versioning](https://semver.org/).

## [0.1.33] - 2026-11-19

### Fixed

- Fixes ZIP import accepts large packs (up to 64 GiB compressed and extracted),
  so full fixes such as the 1.28 GB Assassin's Creed Origins pack no longer fail
  with "archive is too large".

## [0.1.32] - 2026-11-19

### Fixed

- Unlinking an external linked game now keeps its library entry, metadata,
  artwork, and files. Only launchers and the Steam registration are removed;
  use **Remove from library** to delete the entry deliberately.
- Game Settings now refreshes its tabs immediately after linking a launcher, so
  DLC, Updates, Installed Files, and Fixes appear without reopening the dialog.
- Linked games outside `SteamTronLibrary` can receive fixes again. Their fixes
  live under the app-data `fixes/<appid>` folder instead of failing with
  "game is not inside a library folder".
- Closing the main window on Windows hides SteamTron to a tray icon instead of
  exiting; the tray offers Show SteamTron and Quit SteamTron.

## [0.1.31] - 2026-11-18

### Fixed

- Windows registration now writes Lua manifests to `Steam/config/stplug-in`,
  the directory BetterSteamTools watches, instead of falling back to
  `config/lua` where Steam never applied them. Synced games now appear in the
  Steam library without Millennium or the LuaTools plugin.
- Windows registration now also removes the per-game `appmanifest_<id>.acf`
  and `downloading/<id>` for ten seconds after writing the Lua license. Steam
  creates those on hot reload and immediately queues a full game download;
  removing them keeps ownership and the library entry while only the
  Steamworks redistributable may update.
- Windows registration no longer writes depot manifests into Steam's
  `steamapps/depotcache`. Steam treated those as installed depots and queued
  automatic updates (including the ~500 MB Steamworks common redistributables)
  every launch. Existing same-name manifests are removed on the next sync.
- The BetterSteamTools installer now adds scoped Microsoft Defender
  exclusions before downloading, covering its staging folder, three DLLs, and
  the `opensteamtool`/`config/stplug-in` folders. A quarantine during a real
  install previously required restoring the DLL by hand. Game fixes keep their
  existing per-game exclusion flow.
- The Windows backend installer now uses BetterSteamTools v1.0.9 (maintained
  OpenSteamTool fork, 2026-11-13) instead of the June 1.4.8 build, and pins its
  SHA-256. The DLL initializes against the current Steam client; showing
  unlocked games in Steam's library still needs the LuaTools/Millennium plugin.

## [0.1.27] - 2026-11-18

### Fixed

- Doctor repair buttons (for example **Install BetterSteamTools**) no longer
  fail with `ReferenceError: backendName is not defined`; the shared backend
  label is defined once for the whole UI.
- Local cover and hero artwork now loads on Windows. The asset-protocol scope
  used `$LOCALAPPDATA`, which Tauri does not resolve, so every local image URL
  returned 403; the scope is now `$LOCALDATA/steamtron/artwork/**`.

## [0.1.26] - 2026-11-18

### Added

- Windows Play checks the launch target's imports and installs the Microsoft
  runtimes the game needs before starting it: DirectX End-User Runtime (June
  2010) for `XINPUT1_3`, `X3DAudio1_7`, `XAPOFX1_5`, `d3dx9/10/11` and
  `d3dcompiler_43`, plus Visual C++ 2010, 2012, 2013, and 2015-2022 for the
  matching architecture. Installers are downloaded once into
  `%LOCALAPPDATA%\steamtron\runtimes`, checked for a valid Microsoft
  Authenticode signature, and run silently behind one elevation prompt. This is
  the Windows counterpart of Proton's per-game prefix and Steam's install
  script, which managed depot downloads previously skipped.

### Changed

- The per-game "Require Steam to be running" switch is removed from Game
  Settings on Linux and Windows, along with its save button and the
  write-only launch-settings parameter. Libraries that already stored the flag
  keep their existing launch behavior.

### Fixed

- Searching the Store no longer replaces the whole page with "Loading the
  Store…". A new query empties the page cache, and the empty state used to win
  before the IGDB status was known; the sidebar, filters, and results container
  now stay mounted and only the results area shows progress.
- Settings → Storage can no longer sit on "Loading…" forever. The tab bounds
  both IPC calls and renders with fallbacks if a snapshot or drive enumeration
  stalls.
- A game restored from a backup shows its cover and hero without restarting
  SteamTron. The artwork lookup cached the empty result from before the restore
  wrote the image files.

- Windows Play writes the overlay wrapper's start, mount point, working
  directory, launch command, and exit code to the game's diagnostic log.
  Windows games are GUI binaries, so nothing reached stdout/stderr and Game
  Settings → Debug showed "No log yet. Launch the game to create one." even
  though the launch had succeeded.

- Windows Play now layers fixes over the untouched game install with the WinFsp
  union, the same way Linux uses fuse-overlayfs. The union mounts on the first
  free drive from `S:` to `Z:`; directory mount points failed on some hosts with
  `STATUS_OBJECT_NAME_COLLISION` (`0xD0000035`). Managed games no longer need
  the fix DLL copied into the content tree.
- Removing GoldBerg now works when the fixes pack was imported without a
  GoldBerg manifest. The fixes scan stopped six levels deep, so UE4 layouts
  (`Engine/Binaries/ThirdParty/Steamworks/Steamv132/Win64/steam_settings/…`)
  looked applied to the UI but yielded nothing to delete.
- Applying GoldBerg no longer rewrites `steam_settings/steam_interfaces.txt`
  with the emulator's exported symbol names. That list must hold interface
  versions (`SteamUtils007`); overwriting it made GSE report "Missing interface"
  and the game exited after the splash. An existing list is preserved and none
  is written when the fixes pack ships none.
- Steamless and GoldBerg resolve registered launch targets through
  `resolve_executable`, so a library restored from another machine (for example
  `/mnt/krunal/...` stored as `D:/mnt/krunal/...`) no longer fails with
  "No launcher was found inside this game's folder".

## [0.1.25] - 2026-11-17

### Fixed

- The DLC size column no longer wraps `984.3 MiB` onto a second line.
- The Windows bootstrapper no longer extracts already-installed prerequisites
  before skipping them.

## [0.1.24] - 2026-11-17

### Fixed

- Windows VC++ detection now reads `Sysnative` for the x64 runtime DLLs, so the
  32-bit bootstrapper does not mistake WOW64 redirection for a missing runtime.

## [0.1.23] - 2026-11-17

### Fixed

- The DLC tab now uses Steam PICS depot metadata instead of only the Store DLC
  list. DLC without a depot manifest size is hidden, and names/sizes match
  Steam's visible DLC entries.

## [0.1.22] - 2026-11-17

### Fixed

- Settings disables Restart Steam when Steam is not installed instead of
  attempting a restart.
- The Windows bootstrapper reads the native x64 VC++ registry view and checks
  System32, so an installed x64 runtime is no longer reported missing after an
  uninstall.

## [0.1.21] - 2026-11-17

### Added

- Doctor has a Sync Steam library action that registers SteamTron games added
  before Steam or the unlock backend was available. It is enabled only when
  both Doctor checks are healthy and leaves game content untouched.

## [0.1.20] - 2026-11-17

### Changed

- When Steam and the platform backend are both installed, Doctor hides the
  Repair action and leaves only the two read-only status rows.

## [0.1.19] - 2026-11-17

### Changed

- Doctor now shows only the Steam installation check and the platform backend
  check. Process, injection, Lua-directory, appmanifest, and managed-ownership
  rows are removed.
- The backend install action appears only when Steam is installed and the
  backend is missing.
- Linux repair installs only SLSsteam Moon; it does not install Lumen or
  LuaTools.
- Windows Doctor can install the BetterSteamTools/OpenSteamTool runtime from a
  pinned, checksum-verified release using UAC elevation.

## [0.1.18] - 2026-11-17

### Fixed

- Linux Doctor leaves “Repairing…” after the installer finishes instead of
  waiting for background installer processes to release the terminal log pipe.

## [0.1.17] - 2026-11-17

### Added

- Linux Doctor detects a 32-bit SLSsteam Moon hook as incompatible with the
  64-bit Steam client and repairs it by downloading the x86_64 runtime into
  `~/.local/share/SLSsteam`. No SLSsteam, Lumen, or LuaTools files are bundled
  in the application, and managed games missing from Steam are re-registered.
- Doctor no longer offers to install SLSsteam Moon or BetterSteamTools when
  Steam is missing, and disables the Repair action until Steam is found.
- `scripts/uninstall-linux-integration.sh` removes SLSsteam Moon, Lumen, and
  the LuaTools integration while preserving Steam and game Lua scripts.

## [0.1.16] - 2026-11-17

### Fixed

- The Play button now changes to Stop while a managed Windows game is running.
  The run marker was previously removed before the child process exited.

## [0.1.15] - 2026-11-17


### Fixed

- The Windows bootstrapper verifies real .NET Desktop 9, VC++ runtime files, and
  the WinFsp service instead of trusting stale registry/directory entries.
- Missing prerequisites after installation now abort with a clear error unless a
  reboot is required.

## [0.1.14] - 2026-11-17


### Fixed

- The Windows bootstrapper warns and refuses to launch when a prerequisite
  install requires Windows to restart first.

## [0.1.13] - 2026-11-17


### Fixed

- Release CI invokes the installed NSIS compiler by absolute path when building
  the Windows prerequisite bootstrapper.

## [0.1.12] - 2026-11-17


### Fixed

- The Windows bootstrapper compiles without the unsupported `SetProgress`
  instruction and is attached as the primary Windows release asset.

## [0.1.11] - 2026-11-17


### Added

- A Windows bootstrapper checks for Visual C++, .NET 9 Desktop Runtime, and
  WinFsp, installs missing prerequisites silently, then installs SteamTron.
- The bootstrapper uses one elevated installer UI with a prerequisite checklist
  and staged progress.

## [0.1.10] - 2026-11-17


### Fixed

- Windows downloads use SMD's embedded DepotDownloaderMod engine instead of
  the native engine that stalls while fetching depot manifests.
- Windows requires the VC++ x64 and .NET runtime prerequisites.
- The background Store warm-up no longer paints the Store screen while the
  active route is Home.

## [0.1.9] - 2026-11-17


### Fixed

- Windows drive labels no longer expose the `\\?\` canonical prefix or repeat
  `System (C:)` in the destination-drive picker.

## [0.1.8] - 2026-11-17


### Fixed

- Windows managed downloads use SMD's native downloader instead of an
  unavailable DepotDownloaderMod engine, preventing the `Preparing depots…`
  failure.

## [0.1.7] - 2026-11-17


### Fixed

- Windows no longer shows Linux-only Proton Manager, Proton compatibility,
  prefix-source or Feral GameMode controls.
- Windows managed games launch their native executable instead of requiring
  Proton.

## [0.1.6] - 2026-11-17


### Fixed

- The Windows installer copies `winfsp-x64.dll` beside the SteamTron executable
  when WinFsp is already installed, so the normal shortcut launches without a
  separate PATH launcher.

## [0.1.5] - 2026-11-16


### Changed

- Windows installs system-wide under `C:\Program Files (x86)\SteamTron`
  instead of `%LOCALAPPDATA%\SteamTron`; the installer now requests elevation.

## [0.1.4] - 2026-11-16


### Fixed

- Helper processes no longer flash console windows on Windows. Registry,
  tasklist, taskkill, PowerShell and download helpers use `CREATE_NO_WINDOW`.

## [0.1.3] - 2026-11-16


### Fixed

- Windows now lists mounted fixed and removable drives in **Add game drive**,
  including the Windows system volume.
- Windows drive registration now verifies writability and labels the system
  volume instead of showing an empty drive picker.

## [0.1.2] - 2026-11-16


### Added

- How-to-use guide on the website and in the repository README.
- Repository and website release changelogs.
- Release workflow validation that every tag has a matching changelog entry.

### Fixed

- Sign-in now shows a spinner and `Signing in…` while the browser callback is
  pending.
- The blocking OAuth wait no longer freezes the desktop UI.

## [0.1.1] - 2026-11-16

### Added

- Public Linux packages: `.deb`, `.rpm`, and `.AppImage`.
- Windows NSIS installer built by GitHub Actions.
- Direct IGDB Store with 36 server-filtered PC titles per page.
- Per-user IGDB connection page and token refresh.
- Open-source README, API reference, contribution guide, and MIT license.

### Fixed

- Store pages no longer become short or empty after filtering.
- Store now uses direct IGDB requests instead of FastAPI catalog pages.
- Windows builds import the Tauri `Emitter` trait on all platforms.
- Windows drive discovery uses an explicit result type.
- Cached Store pages open without a blocking connection probe.

## [0.1.0] - 2026-11-16

### Added

- Steam library discovery and managed library.
- Per-game fixes, GoldBerg, and Steamless.
- Proton runner management and managed launches.
- DLC, update pinning, backups, saves, diagnostics, and tray support.
- Fullscreen and controller navigation.

[0.1.36]: https://github.com/krunaldodiya/steamtron-desktop/releases/tag/v0.1.36
[0.1.35]: https://github.com/krunaldodiya/steamtron-desktop/releases/tag/v0.1.35
[0.1.34]: https://github.com/krunaldodiya/steamtron-desktop/releases/tag/v0.1.34
[0.1.33]: https://github.com/krunaldodiya/steamtron-desktop/releases/tag/v0.1.33
[0.1.32]: https://github.com/krunaldodiya/steamtron-desktop/releases/tag/v0.1.32
[0.1.31]: https://github.com/krunaldodiya/steamtron-desktop/releases/tag/v0.1.31
[0.1.30]: https://github.com/krunaldodiya/steamtron-desktop/releases/tag/v0.1.30
[0.1.29]: https://github.com/krunaldodiya/steamtron-desktop/releases/tag/v0.1.29
[0.1.28]: https://github.com/krunaldodiya/steamtron-desktop/releases/tag/v0.1.28
[0.1.27]: https://github.com/krunaldodiya/steamtron-desktop/releases/tag/v0.1.27
[0.1.26]: https://github.com/krunaldodiya/steamtron-desktop/releases/tag/v0.1.26
[0.1.25]: https://github.com/krunaldodiya/steamtron-desktop/releases/tag/v0.1.25
[0.1.24]: https://github.com/krunaldodiya/steamtron-desktop/releases/tag/v0.1.24
[0.1.23]: https://github.com/krunaldodiya/steamtron-desktop/releases/tag/v0.1.23
[0.1.22]: https://github.com/krunaldodiya/steamtron-desktop/releases/tag/v0.1.22
[0.1.21]: https://github.com/krunaldodiya/steamtron-desktop/releases/tag/v0.1.21
[0.1.20]: https://github.com/krunaldodiya/steamtron-desktop/releases/tag/v0.1.20
[0.1.19]: https://github.com/krunaldodiya/steamtron-desktop/releases/tag/v0.1.19
[0.1.18]: https://github.com/krunaldodiya/steamtron-desktop/releases/tag/v0.1.18
[0.1.17]: https://github.com/krunaldodiya/steamtron-desktop/releases/tag/v0.1.17
[0.1.16]: https://github.com/krunaldodiya/steamtron-desktop/releases/tag/v0.1.16
[0.1.15]: https://github.com/krunaldodiya/steamtron-desktop/releases/tag/v0.1.15
[0.1.14]: https://github.com/krunaldodiya/steamtron-desktop/releases/tag/v0.1.14
[0.1.13]: https://github.com/krunaldodiya/steamtron-desktop/releases/tag/v0.1.13
[0.1.12]: https://github.com/krunaldodiya/steamtron-desktop/releases/tag/v0.1.12
[0.1.11]: https://github.com/krunaldodiya/steamtron-desktop/releases/tag/v0.1.11
[0.1.10]: https://github.com/krunaldodiya/steamtron-desktop/releases/tag/v0.1.10
[0.1.9]: https://github.com/krunaldodiya/steamtron-desktop/releases/tag/v0.1.9
[0.1.8]: https://github.com/krunaldodiya/steamtron-desktop/releases/tag/v0.1.8
[0.1.7]: https://github.com/krunaldodiya/steamtron-desktop/releases/tag/v0.1.7
[0.1.6]: https://github.com/krunaldodiya/steamtron-desktop/releases/tag/v0.1.6
[0.1.5]: https://github.com/krunaldodiya/steamtron-desktop/releases/tag/v0.1.5
[0.1.4]: https://github.com/krunaldodiya/steamtron-desktop/releases/tag/v0.1.4
[0.1.3]: https://github.com/krunaldodiya/steamtron-desktop/releases/tag/v0.1.3
[0.1.2]: https://github.com/krunaldodiya/steamtron-desktop/releases/tag/v0.1.2
[0.1.1]: https://github.com/krunaldodiya/steamtron-desktop/releases/tag/v0.1.1
[0.1.0]: https://github.com/krunaldodiya/steamtron-desktop/releases/tag/v0.1.0
