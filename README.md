# SteamTron

SteamTron is an open-source desktop manager for Steam libraries, per-game fixes,
managed downloads, Proton runtimes, DLC, updates, backups, and an IGDB-powered
Store.

It is built with Rust and Tauri. The Linux build is the primary target. A
Windows build and WinFsp overlay are included, but the Windows overlay remains
under active hardening.

[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Platforms](https://img.shields.io/badge/platforms-Linux%20%7C%20Windows-2f86c9)](#platform-support)
[![Latest release](https://img.shields.io/github/v/release/krunaldodiya/steamtron-desktop?include_prereleases)](https://github.com/krunaldodiya/steamtron-desktop/releases)

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
[GitHub Releases](https://github.com/krunaldodiya/steamtron-desktop/releases).

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

Before installing SteamTron, install these Windows prerequisites:

1. **Microsoft Visual C++ 2015–2022 x64 Redistributable**
   — <https://aka.ms/vs/17/release/vc_redist.x64.exe>
2. **Microsoft .NET 9 Runtime (x64)**
   — <https://aka.ms/dotnet/9.0/dotnet-runtime-win-x64.exe>
3. **WinFsp 2.1 x64** — <https://winfsp.dev/rel/>. In the WinFsp setup,
   select **Core** only.

The managed downloader uses the VC++ and .NET runtimes. The Windows overlay
needs WinFsp's signed kernel driver; the SteamTron installer copies the WinFsp
DLL beside the app when WinFsp is already installed.

Then run `SteamTron_*_x64-setup.exe`. The installer is system-wide, installs
under `C:\Program Files (x86)\SteamTron`, and requests UAC elevation so it can
copy the WinFsp DLL.

Steam is required for Steam games. WebView2 is included with Windows 11 and
installed by the SteamTron installer when needed.

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

## How to use

### 1. Install and launch

Install the package for your platform from [GitHub Releases](https://github.com/krunaldodiya/steamtron-desktop/releases),
then start **SteamTron**.

- Linux: install the `.deb`/`.rpm`, or run the AppImage.
- Windows: install the three Windows prerequisites below, then run the
  system-wide NSIS installer.

#### Windows prerequisites

| Dependency | Purpose | Link |
|---|---|---|
| Visual C++ 2015–2022 Redistributable (x64) | Required by SMD | <https://aka.ms/vs/17/release/vc_redist.x64.exe> |
| .NET 9 Runtime (x64) | Required by the embedded DepotDownloaderMod engine | <https://aka.ms/dotnet/9.0/dotnet-runtime-win-x64.exe> |
| WinFsp 2.1 (x64) | Signed Windows overlay driver; select **Core** | <https://winfsp.dev/rel/> |

Install all three before launching a managed download. SteamTron currently
requires these prerequisites separately; they are not yet bundled into the
Windows installer.

### 2. Sign in

Click **Sign in**. SteamTron opens the SteamTron website in your browser and
waits for the account login to finish. Return to the app; the sign-in dialog
closes automatically.

If the browser does not open, use the printed URL. After signing in, the
account menu in the top-right shows the signed-in email.

### 3. Confirm Steam and storage

Open **Settings**.

1. Run **Doctor** and resolve any Steam or backend warning.
2. Open **Storage** and add the drive that should hold managed games.
3. Linux users should keep Proton installed. Windows users need WinFsp, the
   VC++ runtime, and the .NET 9 runtime before downloading managed games.

### 4. Add a game

Use one of these paths:

- **Store** — browse or search IGDB, open a PC title, and choose **Add to
  library**. Select a game drive, then wait for the manifest and depot download.
- **Add game** — search the Steam catalogue by title, App ID, or Steam URL.
- **Game Settings → Game Link** — link an already installed external game and
  choose its launcher executable.

If **Connect IGDB** appears in the Store, open **Settings**, connect the IGDB
application, and retry.

### 5. Finish game setup

Open the game and use **Game options**:

- **General** — confirm the launch target, working directory, arguments, and
  optional pre-launch script.
- **Compatibility** — choose Proton, prefix source, and whether Steam must be
  running.
- **Environment** — add per-game environment variables such as
  `WINEDLLOVERRIDES` when required.
- **DLC** — select add-ons, download them, or remove installed content.
- **Fixes** — import fixes, apply GoldBerg/Steamless where appropriate, or open
  the fixes folder.
- **Updates** — inspect available builds and pin or clear a manifest set.
- **Installed Files** — verify content, back up the game, move it to another
  drive, or uninstall it.

### 6. Play

Press **Play** on Home, Library, or the game detail page. SteamTron starts the
configured launcher through the selected Proton runtime. Use **Stop** to end the
owned game processes.

Some Steam registration and configuration actions require Steam to be closed.
SteamTron prompts before those operations.

### 7. Maintain the library

- **Backups** — export game content, fixes, manifests, and save snapshots from
  **Installed Files**.
- **Updates** — pin or clear builds in **Updates**.
- **Remove from library** — removes the Steam registration but keeps game files.
- **Uninstall** — deletes managed game content while keeping the library entry.
- **Diagnostics** — open **Settings → Diagnostic logs** for per-game logs.
- **Controller/fullscreen** — use the mode button, F11, or a connected gamepad;
  Page Up/Page Down moves through collection pages.

### Troubleshooting a first game

- No **Play** button: the game has no launch target; set one under
  **Game Settings → General**.
- Download stops with `VCRUNTIME140.dll was not found`: install the Microsoft
  Visual C++ x64 Redistributable and retry.
- Download stops at `Fetching depot manifests…`: install the Microsoft .NET 9
  Runtime (x64) and retry.
- Windows drive picker is empty: install WinFsp, refresh drives, and use a
  writable non-system volume such as `D:`.
- Download stops: reopen the operation from the download manager and use
  **Resume**.
- Proton errors: install a runner under **Settings → Proton Manager**, then
  select it under **Game Settings → Compatibility**.
- Store is unavailable: check your account session and the SteamTron API
  connection.

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
```

## Website

Visit [steamtron.vercel.app](https://steamtron.vercel.app) for the public
landing page and downloads.

## API

See [API.md](API.md) for the first-party API endpoints, authentication, IGDB
token lifecycle, direct upstream APIs, and error formats.

## Changelog

Every release has a user-facing change list in [CHANGELOG.md](CHANGELOG.md) and
on the [website](https://steamtron.vercel.app/#changelog).

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
