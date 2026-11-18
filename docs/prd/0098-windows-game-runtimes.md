# Windows game runtime provisioning

- Status: active
- Date: 2026-11-18
- Areas: `crates/steamtron-desktop`

## Problem

Linux managed games run inside a Proton/UmuTron prefix. Wine implements the
DirectX and MSVC DLL names as builtins, so a UE4 title that imports
`XINPUT1_3.dll`, `X3DAudio1_7.dll`, `XAPOFX1_5.dll`, `MSVCP140.dll`, and
`VCRUNTIME140.dll` starts without a manual runtime install.

On Windows there is no such layer. Steam runs each app's install script
(`_CommonRedist`) after download; SteamTron's managed downloads only fetch
content depots, and launch-target discovery deliberately skips
`_CommonRedist`. Little Nightmares (424840) therefore failed with "install
DirectX first" until DirectX 9 was installed by hand.

Evidence: `llvm-objdump -p LittleNightmares.exe` lists `XINPUT1_3.dll`,
`X3DAudio1_7.dll`, `XAPOFX1_5.dll`, `d3d9.dll`, `dxgi.dll`, `d3d11.dll`,
`MSVCP140.dll`, and `VCRUNTIME140.dll`. `XINPUT1_3.dll` and `d3dx9_43.dll` were
absent from `System32` before the manual DirectX install.

## Goal

The first Play of a managed Windows game installs only the missing Microsoft
runtimes and then starts the game, without the user finding and running an
installer by hand.

## Non-goals

- Running arbitrary third-party installers from a game folder.
- Changing Linux, Proton, or UmuTron launch behavior.
- Installing every game's complete dependency graph (see residual gaps).
- Bundling redistributables in the SteamTron installer.

## Requirements

- R1: Read the launch target's PE import table before spawn.
- R2: Map DirectX June 2010 imports (`xinput1_3`, `x3daudio1_7`, `xapofx1_5`,
  `xaudio2_7`, `d3dx9_*`, `d3dx10_*`, `d3dx11_*`, `d3dcompiler_43`) to one
  installer.
- R3: Map `msvcp140`/`vcruntime140`/`vcruntime140_1`/`concrt140` to VC++
  2015-2022, and the 120/110/100 families to VC++ 2013/2012/2010, choosing
  x64 or x86 from the PE machine field.
- R4: Check every imported DLL individually; install when any member of a
  component is missing, not only a single probe DLL.
- R5: Download installers on demand into `%LOCALAPPDATA%\steamtron\runtimes`,
  cache them, and reject anything without a valid Microsoft Authenticode
  signature.
- R6: Run installers silently through one UAC elevation per missing component
  and verify the DLLs exist afterwards.
- R7: Log detection, download, elevation, and completion to the game's
  diagnostic log.
- R8: Log known imports with no provisioner (`msvcr90`, `msvcr80`,
  `PhysXLoader`, `PhysXCore`, `nvcuda`, `mscoree`) so a later failure is
  diagnosable.
- R9: Leave the game content untouched.

## Acceptance criteria

- [x] The import reader finds the real DLL names in a 64-bit UE4 game and in a
  system binary.
- [x] Little Nightmares maps to DirectX June 2010 and VC++ 2015-2022 x64.
- [x] Older VC++ families map to their own installers and architecture.
- [x] A DLL present in the game folder or fixes pack counts as satisfied, so
  app-local runtime DLLs are not reinstalled.
- [x] Microsoft-signed binaries pass the signature check.
- [ ] A machine without DirectX/VC++ installs them on first Play. Not
  exercised: this host already has both after the manual DirectX install.

## Verification

- `cargo test -p steamtron-desktop --bin steamtron-desktop runtimes::`:
  7 passed.
- PE import output for Little Nightmares matches `llvm-objdump -p` and maps to
  `DirectX` + `VcRedist { V14, x64 }`.
- Runtime installer URLs returned 200 with the expected sizes:
  `directx_Jun2010_redist.exe` 100,275,120 bytes; `vc_redist.x64.exe`
  25,635,768 bytes; `vc_redist.x86.exe` 13,953,392 bytes. Older VC++ links were
  taken from Microsoft's "Latest supported Visual C++ Redistributable
  downloads" page.
- `Get-AuthenticodeSignature` on a Microsoft system binary returns `Valid` with
  a Microsoft signer, as required by `signature_ok`.
- The installer execution path (download, extract, elevate, silent install) is
  implemented but the elevated install has not been run on this host because
  the runtimes are already present. The download and extraction half was
  exercised: the 100,275,120-byte DirectX package downloaded with a `Valid`
  Microsoft Authenticode signature, and `directx_Jun2010_redist.exe /Q /T:…`
  extracted 157 files including `DXSETUP.exe` with exit code 0.

## Decisions and risks

- Per-import detection avoids false "component installed" results when only
  part of a runtime exists.
- Installers are fetched rather than bundled to keep the installer small; the
  tradeoff is a first-use download (100 MB for DirectX).
- The UAC prompt is unavoidable without a privileged service; the user approves
  once per runtime and the cache makes later plays silent.
- Residual gaps: VC++ 2008/2005, XNA, PhysX, and .NET have no provisioner yet;
  they are logged. Games that ship their own redistributable folder still work
  only if the runtime is already installed or the user runs the game's
  installer.
- Play blocks while provisioning; the play log records progress but the UI
  shows only its normal launch state.

## Links

- Related: `0096-windows-union-overlay`, `0097-windows-launch-log`,
  `0081-windows-vcredist`.
