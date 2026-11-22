# Install a game's bundled PlayStation PC SDK runtime before launch

- Status: active
- Date: 2026-11-22
- Areas: `crates/steamtron-desktop/src/runtimes.rs`,
  `crates/steamtron-desktop/src/main.rs` (comment only)

## Problem

Steam runs each app's install script after download. Ghost of Tsushima's
`installscript.vdf` runs `install_pspc_sdk_runtime.bat quiet`, which installs
the PlayStation(R) PC SDK Runtime (a manager MSI plus a 373 MB runtime MSI)
system-wide.

SteamTron's managed install, download and backup-restore paths copy content
only. `runtimes::ensure_for_game` provisions DirectX and VC++ from the launch
target's PE imports (0098) and deliberately does not run install scripts.

Evidence from 2026-11-22, after restoring Ghost of Tsushima (2215430) from a
backup:

- `GhostOfTsushima.exe` starts, but its helper `crs-video.exe` shows
  `The program can't start because PlayStation(R) PC SDK Runtime is missing
  from your computer.` and dies. The Windows Application log records
  `crs-video.exe` exception `0xc0000409` at 12:34:18 and 12:35:26, one per
  Play, while the game itself keeps running after the dialog is dismissed.
- No `PlayStation(R) PC SDK Runtime` uninstall entry, no
  `%ProgramData%\Sony Interactive Entertainment Inc\PSPC_SDK` folder, and no
  Sony registry keys existed.
- The game folder carried `install_pspc_sdk_runtime.bat`,
  `PsPcSdkRuntimeManager.msi` (product `{491CB67F-…}`, version 2.50.0010) and
  `PsPcSdkRuntimeInstaller.msi` (373 MB, product `{A7B7442A-…}`, version
  2.50.00.10), unrun.
- Running the script elevated installed 748 MB under
  `%ProgramData%\Sony Interactive Entertainment Inc\PSPC_SDK\S22\2.50.00.10`
  (manager product state 5) and the helper no longer fails.

The PE-import scanner cannot see this dependency: `GhostOfTsushima.exe` does
not import a PSPC DLL; the dependency exists only in the install script.

## Goal

The first Play of a game whose folder ships the PlayStation PC SDK runtime
bundle installs it before the launch, without the user finding and running the
installer by hand.

## Non-goals

- Running arbitrary install scripts or `.msi` files from a game folder; 0098
  keeps that as a non-goal and this change uses an allow-list of the known
  PlayStation bundle.
- Parsing `installscript.vdf` in general.
- Provisioning other third-party redistributables (VC++ 2008/2005, XNA, PhysX,
  .NET); they stay in the 0098 residual gap.
- Upgrading an already-installed runtime when a newer game ships a newer MSI.
- Changing DirectX/VC++ provisioning, Linux launches, or the overlay.
- Downloading anything: only files already in the game folder are executed.

## Requirements

- R1: Treat a game as needing the runtime only when
  `install_pspc_sdk_runtime.bat`, `PsPcSdkRuntimeManager.msi` and
  `PsPcSdkRuntimeInstaller.msi` all exist in the game root.
- R2: Treat the runtime as installed when
  `%ProgramData%\Sony Interactive Entertainment Inc\PSPC_SDK\<family>\<version>\runtime.dll`
  exists for any family/version.
- R3: When R1 holds and R2 does not, run `cmd /c "<game>\install_pspc_sdk_runtime.bat" quiet`
  once through the existing elevation helper before the game is spawned; the
  batch file installs the manager and runtime MSIs, so there is one UAC prompt.
- R4: Best-effort: a failed, timed-out or dismissed install is written to the
  play log and never prevents the game from launching, because the runtime is
  optional for gameplay.
- R5: After the attempt, re-check R2 and log whether the runtime was found, so
  a silent installer failure is diagnosable.
- R6: Do not run anything when the bundle is absent (no filesystem or process
  work beyond the checks).
- R7: Keep the check cheap on every launch: two directory scans and three
  `is_file` checks.

## Acceptance criteria

- [x] Unit tests cover bundle detection (all three files required) and runtime
      detection (only a `runtime.dll` under a version folder counts).
- [x] A game without the bundle launches exactly as before: the probe returns
      before any filesystem or process work and no other launch path changed.
- [ ] A restored game with the bundle and no runtime shows one UAC prompt, the
      installed runtime is detected afterwards, and the play log records it.
- [x] The next Play of the same game does not prompt again: the installed
      runtime is detected and the probe short-circuits (detection test plus the
      real `PSPC_SDK` folder on this host).
- [ ] Dismissing the UAC prompt leaves the game launching and logs the reason.

## Verification

- Code: the desktop crate type-checks and codegens with the change
  (`cargo test -p steamtron-desktop --bin steamtron-desktop runtimes::`); the
  final Linux link in this headless environment fails only on GTK
  shared/static libraries that the temporary pkg-config sysroot does not
  provide, which is an environment limitation, not a code error.
- Code: the two new tests were copied verbatim from `runtimes.rs` into a
  std-only harness and built with `rustc --test`:
  `detects_the_playstation_runtime_bundle` and
  `detects_an_installed_playstation_runtime` both pass (2 passed, 0 failed).
- Code: the exact elevated command's argument passing was verified with a probe
  batch that reported `ARG=[quiet]` through `cmd /c "<bat>" quiet`.
- The install command itself was exercised by hand on 2026-11-22: the same
  `install_pspc_sdk_runtime.bat quiet` run elevated installed the manager
  (product state 5) and 748 MB under
  `%ProgramData%\Sony Interactive Entertainment Inc\PSPC_SDK\S22\2.50.00.10`;
  both MSI exit codes were 0 and `runtime.dll` is present.
- Runtime: after that install the user relaunched Ghost of Tsushima on
  2026-11-22 and confirmed it runs without the PSPC dialog. That verifies the
  runtime-install half; the in-app automatic path runs the same command and is
  covered by the detection tests until a machine without the runtime exercises
  it.
- Not run: the automatic path inside a Play (the runtime is installed on this
  host now) and declining the UAC prompt.
- Release 0.1.41 (workflow run 44998445120, Linux and Windows builds green)
  published
  https://github.com/krunaldodiya/steamtron-desktop/releases/tag/v0.1.41 with
  `SteamTron_0.1.41_x64-setup.exe` (SHA-256
  `2d54c0e2499d403a561d22fea2717ebf8c40af8deebe2489209ff9dfc0d3c499`, matching
  the published `SHA256SUMS.txt`), `SteamTron_0.1.41_amd64.deb`,
  `SteamTron-0.1.41-1.x86_64.rpm` and `SteamTron_0.1.41_amd64.AppImage`. The
  installer was downloaded to `C:\Users\krunaldodiya\Downloads` and was not
  installed in this session.

## Decisions and risks

- An allow-list of the known PlayStation bundle keeps 0098's "no arbitrary
  installers" stance while fixing the reported title. The batch file is game
  content and is the same command Steam's install script runs, but executing
  it does grant its MSIs elevation; that is why it is limited to this exact
  file name and its two companion MSIs.
- One UAC prompt is unavoidable without a privileged service. If the user
  declines, the next Play tries again; the game still starts. A future version
  could remember a declined choice.
- Only a missing runtime is installed. If an older runtime exists and a newer
  game needs a newer one, the helper may still report it; upgrading would need
  the MSI version read from the file, which is out of scope.
- The check runs before the DirectX/VC++ import scan so it is not skipped when
  the PE reader cannot open a launcher.

## Links

- `0098-windows-game-runtimes.md` (DirectX/VC++, residual gaps).
- `0092-restart-and-vc-detection.md`.
- Ghost of Tsushima appid 2215430; `crs-video.exe`; `installscript.vdf`.
