# Changelog

All notable changes to SteamTron Desktop are recorded here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and releases use [Semantic Versioning](https://semver.org/).

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
