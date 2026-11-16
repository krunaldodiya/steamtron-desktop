# SteamTron

SteamTron is an open-source desktop manager for Steam libraries, per-game fixes,
managed downloads, Proton runtimes, DLC, updates, backups, and an IGDB-powered
Store.

It is built with Rust and Tauri. The Linux build is the primary target. A
Windows build and WinFsp overlay are included, but the Windows overlay remains
under active hardening.

[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Platforms](https://img.shields.io/badge/platforms-Linux%20%7C%20Windows-2f86c9)](#platform-support)
[![Latest release](https://img.shields.io/github/v/release/krunaldodiya/SteamTron-Desktop?include_prereleases)](https://github.com/krunaldodiya/SteamTron-Desktop/releases)

## Features

- Discover installed Steam games and maintain a separate managed library.
- Add games by Steam name, App ID, or URL; import Lua/manifest ZIPs.
- Manage per-game fixes with a FUSE overlay on Linux or a WinFsp overlay on
  Windows.
- Apply/remove GoldBerg and Steamless tooling.
- Download and verify Steam depot content with DepotDownloaderMod.
- Manage Proton runners, default runners, prefixes, and per-game launch options.
- Configure launch targets, environment variables, pre-launch scripts, and DLC.
- Pin or clear game updates from archived manifests.
- Browse an IGDB Store and add Steam titles to the managed library.
- Back up games, metadata, fixes, manifests, and save snapshots.
- Use keyboard and controller navigation in desktop or fullscreen mode.
- Run SteamTron from the tray with long-running operations and diagnostics.

## Platform support

| Platform | Status | Packages |
|---|---|---|
| Linux x86_64 | Primary | `.deb`, `.rpm`, `.AppImage` |
| Windows x86_64 | Preview | NSIS installer (`*-setup.exe`) |

The Windows overlay requires
[WinFsp](https://winfsp.dev/rel/). A Steam installation is required for Steam
registration, launch-option management, and Steam library discovery.

## Install from a release

Download the latest artifact from
[GitHub Releases](https://github.com/krunaldodiya/SteamTron-Desktop/releases).

### Debian / Ubuntu

```sh
sudo apt install ./SteamTron_*_amd64.deb
```

### Fedora / RHEL / openSUSE

```sh
sudo rpm -i SteamTron-*.x86_64.rpm
# or: sudo dnf install ./SteamTron-*.x86_64.rpm
```

### AppImage

```sh
chmod +x SteamTron_*_amd64.AppImage
./SteamTron_*_amd64.AppImage
```

AppImages need FUSE. If FUSE is unavailable, extract and run:

```sh
./SteamTron_*_amd64.AppImage --appimage-extract
./squashfs-root/AppRun
```

### Windows

Run `SteamTron_*_x64-setup.exe`. The installer is per-user and does not require
administrator access. Install WinFsp separately before using the Windows
overlay.

### Linux runtime dependencies

The package does not hard-code distro-specific dependencies. Install the
WebKitGTK/Tauri runtime for your distribution:

```sh
# Debian / Ubuntu
sudo apt install libwebkit2gtk-4.1-0 libgtk-3-0 libayatana-appindicator3-1

# Fedora
sudo dnf install webkit2gtk4.1 gtk3 libappindicator-gtk3

# Arch
sudo pacman -S webkit2gtk-4.1 gtk3 libayatana-appindicator
```

Steam, Proton, and GameMode are installed separately through their upstream
packages.

## First run

1. Start SteamTron.
2. Sign in with your SteamTron account when prompted.
3. Confirm that Steam is detected under **Settings → Doctor**.
4. Add a game from **Store** or use **Add game** for a Steam title.
5. Choose the compatibility layer and launch target in game settings.
6. Use **Play** only after the game content and launch target are configured.

Some operations require Steam to be closed because Steam rewrites its own
configuration files. SteamTron prompts before those operations.

## Data locations

Linux:

```text
~/.local/share/steamtron/       library, manifests, artwork, logs, tools
~/.config/steamtron/            API endpoint and local configuration
~/.cache/steamtron/             catalogue, ProtonDB, size and update caches
```

Windows:

```text
%LOCALAPPDATA%\SteamTron\       library, manifests, artwork, logs, tools
%APPDATA%\SteamTron\            API endpoint and local configuration
```

Game content lives inside the registered library drive under
`SteamTronLibrary/steamtronapps/`. Removing a game from the library does not
delete its files unless the separate uninstall action is used.

## Build from source

### Common prerequisites

- Rust stable (edition 2024)
- Node.js 20 or newer
- `npx --yes @tauri-apps/cli@2`
- Tauri platform prerequisites:
  <https://v2.tauri.app/start/prerequisites/>

### Linux build

```sh
# Debian / Ubuntu build dependencies
sudo apt install libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev \
  patchelf rpm libfuse2

cargo check --workspace
bash scripts/build-linux.sh
```

Bundles are written to:

```text
target/release/bundle/deb/
target/release/bundle/rpm/
target/release/bundle/appimage/
```

### Windows build

Install Visual Studio Build Tools with the **Desktop development with C++**
workload, WebView2, and WinFsp. Then from a Developer PowerShell:

```powershell
npx --yes @tauri-apps/cli@2 build --bundles nsis
```

The installer is written to `target/release/bundle/nsis/`.

## Tests

```sh
cargo test --workspace
cargo test -p steamtron-desktop
node --check crates/steamtron-desktop/ui/app.js
```

For a faster desktop-only loop:

```sh
cargo check -p steamtron-desktop
cargo test -p steamtron-desktop
```

## Project layout

```text
crates/steamtron-auth       account OAuth/PKCE and token storage
crates/steamtron-core       Steam discovery, config, launch and library models
crates/steamtron-library    managed library, downloads, manifests, backups
crates/steamtron-register   Steam registration backends
crates/steamtron-overlay    Linux FUSE and Windows WinFsp overlay
crates/steamtron-doctor     Steam/backend diagnostics
crates/steamtron-desktop    Tauri commands, UI and release packaging
crates/steamtron-cli        overlay CLI
docs/prd                    one PRD per feature or issue
API.md                      endpoints used by the application
site/                       static landing page for steamtron.vercel.app
```

## Website

The public landing page lives in [`site/`](site/). It is configured for
`https://steamtron.vercel.app` and includes release-driven Linux and Windows
download buttons.

## Repositories

SteamTron is split into two repositories:

| Repository | Visibility | Purpose |
|---|---|---|
| [`SteamTron-Desktop`](https://github.com/krunaldodiya/SteamTron-Desktop) | Public (after release setup) | Rust/Tauri desktop application |
| `steamtron` | Private | Main website and API backend |

The API repository is the existing FastAPI service today and is being renamed
from `UmuTron-FastAPI` to `steamtron`. The planned Next.js fullstack application
will host the public website and the API in that repository. The desktop's API
contract is documented in [API.md](API.md) and is intended to stay stable
through that migration.

The GitHub repository renames and visibility change require maintainer
credentials. Run `scripts/set-github-metadata.sh` after `gh auth login`; it
renames `UmuTron-FastAPI` to `steamtron` (private), renames this repository to
`steamtron-desktop`, and makes only the desktop repository public.

## API

See [API.md](API.md) for the first-party API endpoints, authentication, IGDB
token lifecycle, direct upstream APIs, and error formats.

## Contributing

Read [CONTRIBUTING.md](CONTRIBUTING.md). In short:

1. Create or update a PRD under `docs/prd/`.
2. Make the smallest coherent change.
3. Run the relevant tests and a release build when packaging changes.
4. Commit and push to `main`, keeping the worktree clean.

Bug reports and pull requests are welcome. For a new feature, open an issue or
draft PRD first so the scope and platform impact are clear.

## Legal

SteamTron is not affiliated with or endorsed by Valve, Steam, Twitch, IGDB,
Proton, UMU, WinFsp, or any game publisher. Steam, the Steam logo, and game
artwork belong to their respective owners.

SteamTron does not include game content. Users are responsible for complying
with applicable law and the terms of service of Steam and any other service.
Fixes, registration helpers, and compatibility tooling may affect accounts or
installations; use them at your own risk.

## License

MIT. See [LICENSE](LICENSE).
