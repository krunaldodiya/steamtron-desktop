# Windows prerequisite bootstrap

- Status: active
- Date: 2026-11-17
- Areas: Windows packaging, NSIS bootstrap, first-run prerequisites

## Problem

Windows users must manually install three dependencies before the app is fully
usable:

1. Microsoft Visual C++ 2015–2022 x64 Runtime
2. Microsoft .NET Desktop Runtime 9.0.x x64
3. WinFsp 2.1 x64 (Core)

Missing prerequisites produce misleading failures inside the app (for example
`VCRUNTIME140.dll not found`, a stalled depot download, or a missing WinFsp
driver). A README is not an acceptable installation experience.

## Goal

One Windows setup executable that:

- detects each prerequisite;
- shows a checklist with Installed/Missing state;
- installs only missing items with an Install button;
- then installs SteamTron;
- uses one UAC elevation;
- reports reboot requirements clearly.

This is a bootstrapper, not a change to the Linux package.

## Proposed UX

```text
SteamTron Setup
┌──────────────────────────────────────────┐
│ Prerequisites                            │
│                                          │
│ ✓ Visual C++ x64 Runtime     Installed  │
│ ✗ .NET Desktop 9.0.x x64     Missing    │
│ ✓ WinFsp 2.1 x64             Installed  │
│                                          │
│ [ Install missing prerequisites ]        │
│                                          │
│                         [ Next > ]       │
└──────────────────────────────────────────┘
```

After prerequisites:

```text
Installing SteamTron...
```

Then the normal Finish page and optional launch.

## Packaging

Build a small NSIS wrapper around the existing Tauri NSIS installer. The wrapper
embeds:

| Component | Approx size |
|---|---:|
| Visual C++ 14.x x64 | 25 MB |
| .NET Desktop Runtime 9.0.x x64 | 58 MB |
| WinFsp 2.1.25156 x64 | 2 MB |
| SteamTron Tauri installer | 4.5 MB |

Expected total: roughly 90–100 MB. Bundling keeps installation offline; a
download-on-demand variant can be considered later.

## Detection

- VC++: `HKLM\SOFTWARE\Microsoft\VisualStudio\14.0\VC\Runtimes\x64` `Installed`.
- .NET: `HKLM\SOFTWARE\dotnet\Setup\InstalledVersions\x64\sharedfx\Microsoft.WindowsDesktop.App`
  version 9.0.x, or `dotnet --list-runtimes`.
- WinFsp: `WinFsp.Launcher` service / `WinFsp+*` driver service, or the WinFsp
  registry key.

## Install commands

```text
vc_redist.x64.exe /install /quiet /norestart
windowsdesktop-runtime-9.0-win-x64.exe /install /quiet /norestart
msiexec /i winfsp-2.1.25156.msi /qn /norestart
SteamTron_*_x64-setup.exe /S
```

The wrapper runs elevated, so the inner per-machine installer and the kernel
driver install do not produce additional UAC prompts.

## Requirements

- R1: Clean Windows 11 can install with one executable and one UAC.
- R2: Existing compatible components are not reinstalled or downgraded.
- R3: Missing prerequisites block Next until installed, or clearly warn.
- R4: Exit codes 3010/1641 are reported as a reboot requirement.
- R5: WinFsp license and notices are included.
- R6: Normal Tauri installer remains available for scripted installs.
- R7: The wrapper is code-signed before public release.
- R8: Prerequisite installers run silently; no separate installer UI is shown.
- R9: The wrapper shows inline per-step progress and a final success/failure
  result. Exact percentages are shown only where an installer exposes them;
  otherwise the active step uses an indeterminate indicator.
- R9a: The active step shows an animation, elapsed time, a typical duration, and
  periodic "still installing" reassurance. A details log is available inline.
- R9b: The wrapper keeps the UI responsive while installers run by launching
  them asynchronously and polling process state, rather than blocking the
  progress page with `ExecWait`.
- R10: Reboot-required installers are handled with `/norestart` and a clear
  restart prompt after the app installer finishes.

## Non-goals

- Removing the kernel-driver requirement.
- Bundling WebView2 (Windows 11 includes it; Tauri handles older systems).
- Automatic background elevation outside the installer.

## Acceptance

- [ ] Clean Windows VM: one setup exe installs all missing prerequisites and
  SteamTron.
- [ ] All prerequisites already installed: wrapper reuses them and installs only
  SteamTron.
- [ ] One UAC prompt total.
- [ ] Missing .NET varies from 8 to 9 and is corrected.
- [ ] WinFsp service and driver are present after setup.
- [ ] Reboot-required result is shown correctly.
- [ ] Uninstaller behaves normally.

## Decisions and risks

- A wrapper is preferred over a custom Tauri NSIS template because it keeps
  Tauri's installer logic intact and is easier to maintain.
- Bundle size grows to roughly 100 MB.
- VC++/.NET redistribution must follow Microsoft redistribution terms.
- WinFsp is GPLv3 with a linking exception; commercial bundling needs licence
  review and notices.
- Code signing is required to remove SmartScreen warnings; it is separate from
  this feature.
- The elevated wrapper is the only UAC prompt. Child installers inherit the
  elevated token and must be invoked with quiet/silent switches.
- Silent installers do not expose reliable percentages. Do not fabricate a
  percentage; use a step checklist, active-step spinner, elapsed time and a
  log/details view. If exact progress is required later, replace the NSIS
  progress page with a small bundled native bootstrap UI.
- Bundled installers must be verified by signature/hash before execution; the
  app must not download arbitrary executables at install time.
