# Bootstrapper initializes the plugins directory before extracting the inner setup

- Status: active
- Date: 2026-11-23
- Areas: `crates/steamtron-desktop/windows/bootstrapper.nsi`

## Problem

The Windows bootstrapper reports success but installs nothing when run silently
(`/S`), and the same empty-directory path can turn an upgrade into a no-op.

Evidence from the 0.1.52 bootstrapper:

- Extracted `$PLUGINSDIR/SteamTron-app-setup.exe` is a valid 4,822,569-byte NSIS
  installer and installs 0.1.52 when launched directly.
- Running the released bootstrapper `/S` elevated exits 0 in about 8 seconds but
  leaves the previous version untouched.
- An instrumented copy logs `pluginsdir=` (empty) at the start of the Section.
  `SetOutPath "$PLUGINSDIR"` and the subsequent `File` therefore extract to the
  wrong place, and `ExecWait` on `"$PLUGINSDIR\SteamTron-app-setup.exe"` fails
  with the error flag set. The old executable still exists, so the
  `FileExists`-only success check reports success.
- The same instrumented copy with `InitPluginsDir` extracts and launches a fake
  inner installer from `$PLUGINSDIR` correctly.

In graphical mode the prerequisite page calls `nsDialogs`, which initializes
`$PLUGINSDIR` before the Section runs. Silent mode skips the pages, so the
Section starts with an empty `$PLUGINSDIR`.

## Goal

One setup run, silent or graphical, upgrades an existing install and ends with
the requested version installed; a failed inner launch is reported, not hidden.

## Non-goals

- Changing the inner Tauri installer or the upgrade/uninstall decision it makes.
- Adding new prerequisites or changing their install order.

## Requirements

- R1: Initialize `$PLUGINSDIR` in `.onInit` and at the start of the install
  Section, before `SetOutPath`/`File`.
- R2: Clear the error flag before each inner `ExecWait` and abort with a clear
  message if the inner setup cannot be started.
- R3: After the first inner run, compare the installed Add/Remove Programs
  `DisplayVersion` with the bootstrapper version, not only the executable's
  presence.
- R4: If the version is wrong, retry the inner setup once, then abort with a
  clear message if it is still wrong.
- R5: Preserve the existing prerequisite skip and reboot-required behavior.

## Acceptance criteria

- [x] The bootstrapper compiles with the real 0.1.52 inner and prerequisite
      installers using local NSIS.
- [x] An instrumented bootstrapper with `InitPluginsDir` extracts and launches a
      fake inner installer from `$PLUGINSDIR` in silent mode.
- [ ] The built 0.1.53 setup upgrades 0.1.52 to 0.1.53 in one silent run.
- [ ] The same setup upgrades 0.1.49 to 0.1.53 in one silent run.
- [ ] The graphical wizard still installs/upgrades in one run.

## Verification

- `makensis` compile of the updated `bootstrapper.nsi` with the 0.1.52 inner and
  real `vc_redist`, .NET, and WinFsp payloads: exit 0, 91,460,986-byte output.
- Instrumented copy before the fix: `pluginsdir=` empty, `exec-inner-error-flag=set`,
  no inner log.
- Instrumented copy with `InitPluginsDir`: `pluginsdir=C:\Users\...\Temp\nsrXXXX.tmp`,
  fake inner log records `cmdline="...\SteamTron-app-setup.exe" /S`.
- Not run yet: installed 0.1.53 upgrade (needs the next approved build and UAC).

## Decisions and risks

- `InitPluginsDir` is idempotent, so calling it in `.onInit` and again in the
  Section is safe and covers both silent and graphical flows.
- The version check reads
  `HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\SteamTron`
  (`DisplayVersion`) with the 64-bit registry view. The inner installer writes
  this key per machine.
- If the inner installer succeeds but Windows blocks the registry write, the
  bootstrapper retries once and then reports failure; it no longer claims
  success.
- A message box during a silent install still requires a click; this only
  happens on a real failure, and the previous behavior was a false success.

## Links

- PRD 0131 (bootstrapper upgrade retry)
- PRD 0132 (bundled runtime redistributables)
- Commit: 0.1.53 release
