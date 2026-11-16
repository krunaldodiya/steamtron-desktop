# Built-in WinFsp bootstrap on Windows

- Status: draft
- Date: 2026-11-16
- Areas: Windows packaging, NSIS installer, first-run overlay setup

## Problem

Windows users must install WinFsp separately before SteamTron will start. The
executable imports `winfsp-x64.dll` at load time, and the overlay needs the
signed WinFsp kernel driver/service before a drive or overlay can mount.

Linux has FUSE in the kernel; Windows requires a kernel filesystem driver. A
plain DLL copy is not enough to make the overlay work.

## Goal

One SteamTron installer provides the complete Windows overlay runtime and
requires at most one UAC elevation, with no separate WinFsp installation step.

## Scope

- Detect an existing compatible WinFsp installation.
- Bundle the official WinFsp x64 MSI/runtime with the Windows package.
- Install/repair WinFsp silently during SteamTron installation with elevation.
- Ensure `winfsp-x64.dll` is discoverable by the app at process start.
- Preserve uninstall/repair behavior and do not remove a shared WinFsp install
  that other applications may use.

Non-goals:

- Avoiding Windows kernel-driver installation or UAC entirely.
- Supporting WinFsp on unsupported Windows versions.
- Redistributing WinFsp without licence review.

## Requirements

- R1: A clean Windows machine can install SteamTron and launch it without a
  manual WinFsp download.
- R2: Overlay mount works after installation without a manual PATH edit.
- R3: An existing WinFsp version is left alone when compatible.
- R4: Failure to install WinFsp produces a clear, actionable error.
- R5: The installer requests elevation only for the driver step.
- R6: Include WinFsp licence/notice files in the package.

## Acceptance criteria

- [ ] Clean Windows VM: install SteamTron, launch, sign in.
- [ ] Mount a game overlay/managed drive without installing WinFsp separately.
- [ ] Existing WinFsp install is detected and not downgraded.
- [ ] Uninstall does not break another app that uses WinFsp.
- [ ] Document the UAC prompt and signed-driver requirement.

## Verification

- Windows QEMU VM, clean snapshot.
- Installer logs confirm the WinFsp MSI result.
- Overlay mount and file copy-up tested.
- Repeat with an existing WinFsp installation.

## Decisions and risks

- WinFsp is a signed kernel driver; this cannot be a pure user-level
  dependency-free feature like Linux FUSE.
- Bundling requires licence review before distribution.
- The NSIS install mode may need `perMachine` or an elevated helper. This
  changes the current per-user installer behavior.
- The app should not silently elevate without a visible installer/UAC step.
