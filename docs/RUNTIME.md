# Managed runtime: launch planning, overlay, reconciliation

Status: phase 3 + managed overlay slice · 2026-11-09
Code: `steamtron-core::runtime`, `steamtron-desktop::play_managed`, `steamtron-overlay` wrapper

## Launch plan

`managed_launch_plan(game, target, steam_root)` returns a plain process
description after validating the game:

| Field | Value |
|---|---|
| `program` | Proton runner from `launch.proton` or `installation.proton` |
| `arguments` | `waitforexitandrun <executable> <target arguments>` |
| `environment` | `STEAM_COMPAT_DATA_PATH=<prefix>`, `STEAM_COMPAT_CLIENT_INSTALL_PATH=<steam root>`, `SteamAppId`/`SteamGameId` when an App ID exists |
| `cwd` | game root |

The prefix directory is created before launch. Missing content, executable,
runner or prefix fail with explicit errors; a game without a target cannot be
played yet.

## Fixes overlay

`play_managed` uses the fixes overlay when `fixes_enabled` is set and the game's
fixes folder (`<drive>/SteamTronLibrary/steamtronapps/fixes/<Game>`) is
non-empty:

```text
steamtron-desktop --steamtron-overlay --appid <id> --common <root> \
  --fixes <fixes> --proton-path <proton> --compat <prefix> \
  --cwd <root> --steam-root <steam> -- <exe> <args...>
```

The wrapper mounts `lowerdir=<fixes>:<root>` with fuse-overlayfs, rewrites
arguments and the working directory into the merged view, runs
`<proton> waitforexitandrun <merged exe>`, and unmounts on exit. Managed mode
skips the legacy Steam `localconfig` target lookup; the target comes from the
library entry.

Verified on this workstation with a fake Proton (`/bin/echo`): the merged path
was passed to the command and the mount was removed afterwards.

## Diagnostics

Every managed launch writes `diagnostics/<game id>/play-<epoch>.log` under the
data home (`File::create`, stdout+stderr). The UI toast includes the log path.
The child is reaped on a background thread.

## Registration reconciliation

`reconcile_registrations` verifies every stored Steam registration:

- lua present → `steam.state = registered`;
- lua missing → state stays `pending_restart` and the count is reported;
- backend errors are collected without touching other games.

The desktop calls it on load; `steamtron-overlay managed reconcile` runs it from
the CLI.

## CLI

```bash
# Plan only
steamtron-overlay managed play 3468650 --dry-run

# Start with logging
steamtron-overlay managed play 3468650

# Pick a non-primary target
steamtron-overlay managed play 3468650 --target t2
```

## Limits

- Steam-required launch mode (`steam.launch_mode = steam_client`) is not wired
  yet; managed Play always uses the Steam-free Proton path.
- Newly added games have no launch target until the setup step records one.
- The overlay requires `fuse-overlayfs` and `fusermount3`; without them managed
  Play falls back to the direct Proton plan only when fixes are disabled or the
  fixes folder is empty.

## Supervision and Stop

- Managed launches start a new process group on Unix (`process_group(0)`), are
  registered by game id, and are removed when the child exits.
- `Stop` sends SIGTERM to the group, waits up to 8 seconds for the exit, then
  escalates to SIGKILL. Windows uses `taskkill /T /F`.
- The UI shows Stop while the game is running, refreshes state on window focus,
  and refuses a second Play for the same game.
- Verified: process-group SIGTERM test in `steamtron-core`; Stop wiring builds
  and parses.

## Managed fixes

- The canonical folder is `<drive>/SteamTronLibrary/steamtronapps/fixes/<Game>`; it
  is derived from the game root, never typed by the user.
- `install_fixes` extracts a ZIP there with the existing safety rules (no
  symlinks, size/file caps, staging beside the target so cross-drive renames
  cannot fail) and replaces the previous content.
- `fixes_view` lists installed files (recursive, capped); `remove_fixes` deletes
  the game's fixes folder only.
- Desktop: the managed preview shows the fixes path and file count with
  `Install ZIP…` and `Remove`.
- CLI:

```bash
steamtron-overlay managed fixes list 3468650
steamtron-overlay managed fixes install 3468650 fixes.zip
steamtron-overlay managed fixes remove 3468650
```

Verified end-to-end in the isolated tree: install wrote `fix.dll` and
`readme.txt`, the view listed both, and remove deleted the folder.

