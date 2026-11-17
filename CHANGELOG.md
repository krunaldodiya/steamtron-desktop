# Changelog

All notable changes to SteamTron Desktop are recorded here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and releases use [Semantic Versioning](https://semver.org/).

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
