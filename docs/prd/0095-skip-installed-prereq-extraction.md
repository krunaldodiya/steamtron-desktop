# Skip extracting installed prerequisites

- Status: done
- Date: 2026-11-17
- Areas: `crates/steamtron-desktop/windows/bootstrapper.nsi`

## Problem

The bootstrapper extracted VC++, .NET, and WinFsp installers into `$PLUGINSDIR`
before checking whether they were already installed. The progress page showed
all three extraction steps even when every prerequisite was present.

## Goal

Installed prerequisites are not extracted. Only missing prerequisites and the
SteamTron inner installer are unpacked.

## Requirements

- R1: Run prerequisite detection before extraction.
- R2: Extract each prerequisite only when its status is `Missing`.
- R3: Always extract the inner SteamTron installer.
- R4: Keep prerequisite installation behavior and exit-code handling.

## Verification

- NSIS source wraps each prerequisite `File` command in its status check.
- 0.1.25 Windows release workflow compiled the bootstrapper successfully.
- Windows VM not exercised here.

## Decisions and risks

- `File` runs at installer runtime inside the LogicLib condition; it is not
  compile-time extraction into the application directory.
- Detection runs once on the prerequisites page and again at the start of the
  install section so a changed system state is respected.
