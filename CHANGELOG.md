## [0.1.72] - 2026-11-24

### Fixed

- Normal Play now mounts the real-path overlay directly at the game's real
  folder when WinFsp Mount Manager is available, instead of mounting at
  `.steamtron-mount` and adding a second junction. Launchers that compare the
  final path with the install path (Grand Theft Auto V Enhanced's RUNE build)
  no longer restart in a loop. The real path is restored as a junction to
  `.steamtron-base` after the run.
- Defender exclusions now include the game's `.steamtron-base` and
  `.steamtron-mount` paths, so fix executables served through the overlay are
  not quarantined.
- Grand Theft Auto V Enhanced is no longer blocked by `requires_patch`; the
  imported RUNE fix runs through the overlay.

## [0.1.71] - 2026-11-24

### Fixed

- Real-path overlays now mount through the Windows Mount Manager when
  available. Games and launchers see a real volume identity, so
  `GetFinalPathNameByHandle` and volume queries work. This fixes the Grand
  Theft Auto V Enhanced launcher restart loop (60+ processes, no window).
  The Windows bootstrapper enables WinFsp's non-elevated Mount Manager setting
  when it is absent; the existing DefineDosDevice/junction mount remains as a
  fallback.
- Overlay diagnostics record `mount_mode=mount-manager` or `mount_mode=legacy`
  so a failed launch can be diagnosed without rebuilding.

## [0.1.70] - 2026-11-24

### Fixed

- The GUI no longer stays elevated when it is started by the installer Finish
  page or an administrator shell. It relaunches through Explorer as the
  signed-in user and exits the elevated instance. Per-game overlay and runtime
  children keep their own elevation behavior.

## [0.1.69] - 2026-11-24

### Fixed

- Stopping a managed game now points the real-path junction back to
  `.steamtron-base` after the overlay wrapper is killed. A forced Stop used to
  leave the junction on the unmounted `.steamtron-mount`, so the game appeared
  as 0 B/Download until the next Play.
- Library load recovers a stale persisted root from the canonical
  `common/<title>` folder (for example `common/FarCry5` vs
  `common/Far Cry 5`) instead of reporting missing content.
- Overlay launches use the resolved executable's directory as the working
  directory. Imported games with an old absolute target (Little Nightmares I
  and II) previously got a stale cwd and did not start even though the exe was
  rebased to `S:`.

## [0.1.68] - 2026-11-24

### Fixed

- Opening **Settings → Storage** no longer blocks the whole pane while the
  Windows drive list is enumerated. The library snapshot and drive list are
  prefetched at launch, cached, and rendered immediately; a slow or pending
  drive enumeration only fills the drive rows when it finishes.
- A stale appmanifest `content_directory` no longer replaces a valid library
  root. For example, `common/FarCry5` is ignored when the real folder is
  `common/Far Cry 5`, so Play and the installed size work again.
- `Game::appmanifest` never derives a folder name containing `/`, preventing a
  nested `common/common/<Game>` path from being written by a legacy malformed
  root.

## [0.1.67] - 2026-11-24

### Fixed

- Windows managed games whose library root uses the `\\?\` extended prefix now
  normalize `/` to `\` before the path is made verbatim. A manifest
  `content_directory` such as `common/Max Payne 3` previously produced
  `\\?\D:\...\common/Max Payne 3`, which Win32 rejects with
  `ERROR_INVALID_NAME`. This fixes the `game content folder does not exist`
  error, the `0 B` installation size, and the misleading "base folder is
  missing" message for every affected game. Games without a `.steamtron-base`
  sidecar now show Play again instead of Download when their files exist.
- `repair_game_root` normalizes separators before deriving its parent and name,
  and path equality checks ignore the `\\?\` vs plain representation.

## [0.1.66] - 2026-11-24

### Fixed

- Only one SteamTron instance runs. Launching the app again shows and focuses
  the existing window instead of creating another process and tray icon.
- Added a per-game path diagnostic at
  `<data home>/diagnostics/path-check.log`, written at startup and around
  real-path preparation, so Windows path failures can be inspected even when
  the app was launched elevated.

## [0.1.65] - 2026-11-24

### Fixed

- Before launch planning, a missing or broken game root is repaired from the
  `.steamtron-base` sibling even when the real-path record is gone. If the base
  is also missing, the error names the base folder instead of reporting a
  generic missing content path.

## [0.1.64] - 2026-11-24

### Fixed

- A game manifest that still contained both `launcher_targets` and the legacy
  `launch_targets` could select an older launcher. The resolver now prefers the
  list whose primary executable matches `launcher_relative_path`, and saving
  removes the legacy key.
- A real-path junction left pointing at a missing overlay mount is repaired to
  the base folder before launch planning. A missing base now reports a clear
  error instead of `game content folder does not exist`.

## [0.1.63] - 2026-11-24

### Added

- The per-game `appmanifest_<id>.json` is now the authoritative settings file
  for a managed game. Launchers, Proton, prefix mode, environment, arguments,
  prelaunch and fixes are read from it on every library load and written back
  on every save. `library.json` remains the index/cache, global settings and
  fields that are not per-game settings.

### Fixed

- Restoring a game backup no longer fails with `Access is denied (os error 5)`
  when the destination is a real-path junction. Restore now puts a valid base
  back in place, or removes a stale junction and its layout record, before the
  commit rename.
- Restore keeps an existing configured launcher when its executable still
  exists. Backup targets are fallback only, so a stale snapshot cannot replace
  a working target and send the game back to its original launcher.
- Backups are built from the live game config, including every launch setting,
  instead of copying an older manifest snapshot.
- Storage settings show the data folder and the `library.json` file separately
  on Windows; opening the data folder no longer fails with
  `folder does not exist`.

## [0.1.62] - 2026-11-23

### Added

- Fixes import now accepts ZIP as well as TAR, extracts nested directories and
  rejects unsafe archive paths. The GTA V ZIP's `steam_api64.dll` and
  `steam_settings/` are no longer lost.
- A data-driven launcher catalog supplies the target executable, arguments,
  overlay mode and module hints per App ID. The catalog can mark a game as
  needing a matching game-specific patch instead of starting a known loop.
- The Fixes pane shows the launcher profile and module statuses, with an
  `Apply recommended` action for GoldBerg/Steamless.
- A Windows launch watchdog aborts a launcher that repeatedly respawns its
  process tree instead of leaving the app on Stop indefinitely.

## [0.1.61] - 2026-11-23

### Fixed

- Auto-GoldBerg no longer stacks on a fix that already ships its own Steam
  emulator. RUNE/`version.dll` proxies, Steam client DLLs, CreamAPI/SmartSteamEmu
  configs and existing `steam_settings` are detected before applying GoldBerg.
  This matters for GTA V Enhanced, whose RUNE fix supplies `RUNE64.dll` and
  `version.dll` plus a SocialClub emulator.

## [0.1.60] - 2026-11-23

### Fixed

- Games whose launcher requires administrator rights now work from a normal,
  non-elevated SteamTron. When the game spawn returns `ERROR_ELEVATION_REQUIRED`
  the overlay wrapper restores the junction, unmounts, and relaunches itself
  elevated before mounting again. The elevated wrapper starts the game with the
  correct merged working directory, so Windows no longer rejects the elevation
  because the cwd is on the WinFsp union.
- Overlay wrappers write diagnostics through `--log`, so a relaunched elevated
  wrapper keeps its play log.
- An installed game no longer briefly shows Download while Play is swapping the
  real-path junction; the recorded base counts as installed content, and a
  junction left pointing at an unmounted union is repaired when no game owns it.

## [0.1.59] - 2026-11-23

### Fixed

- Games whose launcher executable requires administrator rights now start
  through a single Windows UAC prompt instead of failing with
  `ERROR_ELEVATION_REQUIRED` (os error 740). Managed Play keeps the real-path
  overlay mounted while the elevated process tree runs, and the overlay
  wrapper still restores the junction and unmounts if elevation is declined.
  Normal games keep the direct launch path.
- Play auto-applies GoldBerg for a launcher fix when the game has a Steam API
  DLL and the fix does not already ship a Steam emulator. Imported fixes are
  kept; GoldBerg uses its own manifest and can still be removed separately.

## [0.1.58] - 2026-11-23

### Added

- Managed Play auto-prepares the real-path overlay for games whose fixes
  contain a launcher module (Uplay, SocialClub, RGL, EA/Origin). It moves the
  game to a base on the same volume, creates the mountpoint and real-path
  junction, and records the layout internally. No manual per-game sidecar is
  needed. Simple games keep the drive-letter overlay. Import/Export/Remove
  fixes are unchanged and remain additive.
- The Fixes pane shows the prepared real-path layout and offers Restore game
  folder. Uninstall and Move restore the layout before touching the game
  folder, so a junction is never mistaken for the game content.

## [0.1.57] - 2026-11-23

### Fixed

- Managed Play now passes the launch target’s own directory as the overlay
  working directory instead of the game root. Max Payne 3’s unprotected
  launcher crashed with `0xC0000005` when started from the root; the working
  manual launch ran it from `…\Max Payne 3\Max Payne 3`.

## [0.1.56] - 2026-11-23

### Added

- Managed Play can use a real-path WinFsp overlay layout (local mountpoint,
  junction at the real game path, pristine base) from a per-game sidecar. The
  overlay launcher receives `--mountpoint`, `--junction` and `--base`
  automatically, so launcher-protected games can use their real install path
  from the Play button. Sidecar creation/removal UI is a follow-up.

## [0.1.55] - 2026-11-23

### Fixed

- The real-path WinFsp directory mount now handles
  `STATUS_OBJECT_NAME_COLLISION`: if the empty mountpoint already exists it is
  removed and the mount is retried (and the extended path form is tried as a
  final fallback). This fixes the 0.1.54 directory mount failure.

## [0.1.54] - 2026-11-23

### Added

- An experimental real-path WinFsp overlay for launcher-protected games
  (Rockstar RGL, Ubisoft Connect, EA/Origin). The union can mount at a local
  empty directory; the game's real path is a junction that normally points at
  the pristine base and, during Play, at the overlay. The volume reports
  NTFS-like metadata, and the junction is restored around the process tree.
  The installed game files are still never overwritten.

## [0.1.53] - 2026-11-23

### Fixed

- The Windows bootstrapper initializes its temporary plugins directory before
  extracting the inner setup. Silent installs and upgrades (`/S`) no longer
  report success while installing nothing, and a leftover older executable is
  detected by comparing the installed version instead of only checking that the
  executable exists. A failed inner launch is now reported, and a single retry
  completes the upgrade.

## [0.1.52] - 2026-11-23

### Fixed

- Download progress now aggregates every depot and uses the real download size.
  The total comes from the expected Steam size (or the appmanifest and merged
  depot plans), never from a single `Downloaded` value, and 100% is held until
  every content group completes. Max Payne 3 no longer shows 100 MiB/100% for a
  32.5 GiB download.
- Windows runtime provisioning prefers the Microsoft-signed redistributables
  shipped with the game (Watch Dogs 2 `_Redist` VC++ 2012) before downloading.
  Invalid cached downloads are retried, and reboot-required exits are accepted.
- The Windows bootstrapper re-runs the inner installer when an upgrade's first
  pass only removed the previous version, so one setup run completes upgrades.

## [0.1.49] - 2026-11-23

### Added

- Restored **Generate app manifest** next to **Generate Lua/manifests** in
  Game Settings → Installed Files. It rewrites the game's appmanifest from the
  current library entry (launch options, targets, paths) and preserves extra
  fields such as details, integrity and quicklook, without Steam or network
  access.

## [0.1.48] - 2026-11-23

### Fixed

- Game-level launch options (stored as `launch_options` in the appmanifest) are
  now passed to the game along with the selected target's arguments on native
  Windows, Proton, and overlay launches. Previously they were saved but ignored,
  so `-nolauncher` never reached Rise of the Tomb Raider.

## [0.1.47] - 2026-11-23

### Fixed

- Restored games, and linked games moved onto a registered drive, are now
  managed based on their actual content path under
  `SteamTronLibrary/steamtronapps/common`. Installed Files shows Uninstall (not
  Unlink) and the uninstall guard permits the deletion.
- Installed Files now renders only the applicable action: Unlink for external
  linked games, Uninstall for managed games.

## [0.1.46] - 2026-11-22

### Fixed

- Restore accepts legacy format-1 backups (no `backup_format`, e.g.
  Mafia: Definitive Edition) and incomplete format-2 manifests. It derives the
  content archive/component, content directory, manifest bundle, and fixes
  archive from the files actually present. The restored library appmanifest is
  normalized; the backup folder is never rewritten.
- A cyclic redundancy check (os error 23) now names the damaged backup component
  and tells the user to re-export to healthy storage. Max Payne 3's
  `content.tar` is damaged on the backup drive; its small components match
  their checksums.

## [0.1.45] - 2026-11-22

### Fixed

- Restore now actually calls the appmanifest finder introduced in 0.1.44. That
  release added the helper and its test but kept the old inline finder, so a
  backup with an `appmanifest_*.cache.json` sidecar (The Witcher 3) still failed
  with "Unsupported SteamTron backup format".

### Added

- An About section at the bottom of Settings → General showing the installed
  SteamTron version.

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
