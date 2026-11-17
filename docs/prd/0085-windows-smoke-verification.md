# Windows smoke verification

- Status: done
- Date: 2026-11-17
- Areas: Windows desktop build, WinFsp overlay, managed downloads

## Environment

- Windows 11 Pro 25H2, build 26300, QEMU/KVM
- SteamTron 0.1.10
- 3 vCPU, 7.5 GiB RAM
- 32 GB data disk formatted NTFS as `D:`
- Visual C++ 2015–2022 x64 Redistributable
- .NET 9 Desktop Runtime (9.0.x, x64)
- WinFsp 2.1.25156 x64, Core feature
- Backend: authenticated Ryuu manifest proxy

## Evidence

- Windows installed SteamTron under `C:\Program Files (x86)\SteamTron`.
- `.NET` runtime check: `Microsoft.NETCore.App 8.0.34` before installing .NET 9;
  DDM 3.4.0 requires .NET 9.
- WinFsp registered:
  - service `WinFsp.Launcher`
  - driver service `WinFsp+2026...`
- The Binding of Isaac: Rebirth (`113200`) manifest ZIP was returned through
  the FastAPI endpoint.
- Managed game content downloaded successfully after using DepotDownloaderMod
  and .NET 9 Desktop Runtime.
- Imported the local fixes ZIP through **Game options → Fixes**.
- The game launched successfully from SteamTron after fixes were applied.

## Remaining checks

- Uninstaller cleanup.
- Multiple games and DLC.
- Overlay whiteouts, opaque directories and copy-up edge cases.
- GPU passthrough remains untested and is not required for app QA.
