# Windows launch diagnostic log

- Status: done
- Date: 2026-11-18
- Areas: `crates/steamtron-desktop`

## Problem

Game Settings → Debug on Windows showed "No log yet. Launch the game to create
one." while `%LOCALAPPDATA%\steamtron\diagnostics\424840\` contained a
`play-*.log` for every launch, all zero bytes. The viewer reads the newest
`play-*.log`; it was empty.

Play creates the file and passes it as stdout/stderr to the overlay wrapper, but
the wrapper only wrote on failure. Windows managed games are GUI executables:
the game writes nothing, and the wrapper emitted nothing on the success path.
On Linux the Proton/Wine chain produces output, so the same code path looks
healthy there.

## Goal

Every Windows overlay launch leaves a readable diagnostic log showing what the
wrapper mounted and executed, even when the game itself prints nothing.

## Non-goals

- Streaming per-frame game output.
- Changing the diagnostics directory or 64 KiB tail limit.
- Installing a logging framework or adding dependencies.

## Requirements

- R1: Log the appid, common root, and fixes pack at wrapper start.
- R2: Log the merged mount path after a successful mount.
- R3: Log the working directory and the rewritten command line.
- R4: Log the game's exit code after it finishes.
- R5: Keep the existing failure line for mount/launch errors.
- R6: Do not change launch behavior or arguments.

## Acceptance criteria

- [x] A Windows overlay wrapper run produces a non-empty log.
- [x] The log contains the start, merged path, working directory, command, and
  exit code.
- [x] The Debug viewer's input (newest `play-*.log`) is non-empty, so it no
  longer falls back to "No log yet".

## Verification

- Ran the installed binary directly with `--steamtron-overlay` against the real
  fixes pack and a harmless `cmd.exe /c echo` command, stdout/stderr redirected
  as Play does. The 345-byte output was:

  ```
  SteamTron overlay: start appid=424840 common=D:\...\Little Nightmares fixes=D:\...\Little Nightmares
  SteamTron overlay: merged=S:\
  SteamTron overlay: cwd=S:
  SteamTron overlay: exec=C:\Windows\System32\cmd.exe /c echo overlay-log-test
  overlay-log-test
  SteamTron overlay: exit=0
  ```

- Pre-change evidence: `diagnostics\424840\play-*.log` existed for every launch
  and every file was 0 bytes.
- Installed build replaced: `ec84d69f7d76aba81b9156708c139024cfce601cea0819931b540d450b321b8e`
  at 17:11, after the change.
- No UI code changed. `managed_game_log` already returns the newest
  `play-*.log` content and the Debug pane renders any non-empty string; the
  missing data was the defect.
- User confirmed after the install that the Debug logs are working.

## Decisions and risks

- Lines go to stderr because Play already redirects both streams to the log;
  no new file handle or logger is needed.
- The wrapper log is capped by the existing 64 KiB tail, so a chatty game can
  still push the wrapper lines out of view.

## Links

- Related: `0096-windows-union-overlay`.
