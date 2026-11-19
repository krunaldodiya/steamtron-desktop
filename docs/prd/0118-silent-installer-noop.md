# Silent installer exits without installing

- Status: draft
- Date: 2026-11-19
- Areas: `crates/steamtron-desktop/windows/bootstrapper.nsi`,
  `crates/steamtron-desktop/windows/installer-hooks.nsh`,
  `.github/workflows/release.yml`

## Problem

The published 0.1.38 Windows bootstrapper does nothing when run silently.

On 2026-11-19, `SteamTron_0.1.38_x64-setup.exe /S` (elevated) was started three
times: 18:03:09, 18:04:08, and 18:05:51 (the last two after closing the running
app and after removing a stale per-machine install). Each run exited `0` after
11–12 seconds, yet:

- `C:\Program Files (x86)\SteamTron` was not written (`LastWriteTime` stayed at
  the 18:02:42 uninstaller run; no `steamtron-desktop.exe`),
- no uninstall registry entry was created,
- no Start Menu shortcut was created.

Running the same downloaded file interactively (18:07:03) installed 0.1.38
successfully: `steamtron-desktop.exe` FileVersion/ProductVersion `0.1.38`,
`uninstall.exe` written at 18:07:23, uninstall entry `SteamTron 0.1.38`,
shortcuts created.

A background watcher that polls `%TEMP%` for the extracted
`SteamTron-app-setup.exe` saw it only during the interactive run. The bootstrapper
logs no failure and reports success, so unattended deployment cannot tell that
nothing happened.

## Goal

`SteamTron_<version>_x64-setup.exe /S` installs exactly like the interactive
wizard: prerequisites are installed when missing, the app lands in
`C:\Program Files (x86)\SteamTron`, the uninstall entry and shortcuts exist,
and failures produce a nonzero exit code and a log line.

## Non-goals

- Changing the interactive wizard.
- Changing the per-machine install location or prerequisites list.
- Adding an MSI or a package-manager manifest.

## Requirements

- R1: `/S` executes the `Install` section, including the inner Tauri installer
  (`SteamTron-app-setup.exe /S`), not just the prerequisite checks.
- R2: `/S` never waits on a modal `MessageBox`; failures are reported with a
  nonzero process exit code and a line in an install log under
  `%LOCALAPPDATA%\SteamTron\diagnostics` (or `$PLUGINSDIR` when the data home
  is unavailable).
- R3: `/S` is idempotent: an existing install is replaced or upgraded, and a
  stale uninstall entry with a missing `uninstall.exe` does not abort the run.
- R4: The release workflow installs the produced bootstrapper silently on
  `windows-latest` and asserts the installed exe's `FileVersion` equals the
  tag, so this cannot regress unnoticed.

## Acceptance criteria

- [ ] On a clean Windows runner, `<setup>.exe /S` exits `0` and creates the app
      exe, uninstall entry, and shortcuts.
- [ ] On a machine with the stale 0.1.34/0.1.25 install, `/S` upgrades to the
      new version without manual cleanup.
- [ ] A forced failure (for example a locked target file) returns a nonzero
      exit code and writes the reason to the install log.
- [ ] CI performs the silent install and version assertion on every tag.

## Verification

Pending. The 0.1.38 install on the development machine used the interactive
wizard; the silent path remains broken.

## Decisions and risks

- The root cause is not yet known. Candidates to check first: the custom page
  callback under `SilentInstall silent`, `SetShellVarContext`/`$INSTDIR` handling
  in `NSIS_HOOK_PREINSTALL`, and Tauri's own previous-install detection.
- Adding a CI silent-install step is the durable guard; a manual retest alone
  let this reach a release.

## Links

- Release `v0.1.38` (workflow run `44519679566`).
- `docs/prd/0117-fixes-launcher-overlay-lifetime.md` (release notes).
