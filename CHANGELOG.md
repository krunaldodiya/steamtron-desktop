# Changelog

All notable changes to SteamTron Desktop are recorded here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and releases use [Semantic Versioning](https://semver.org/).

## [0.1.2] - 2026-11-16

### Added

- In-app How to use guide and website first-run walkthrough.
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

[0.1.2]: https://github.com/krunaldodiya/steamtron-desktop/releases/tag/v0.1.2
[0.1.1]: https://github.com/krunaldodiya/steamtron-desktop/releases/tag/v0.1.1
[0.1.0]: https://github.com/krunaldodiya/steamtron-desktop/releases/tag/v0.1.0
