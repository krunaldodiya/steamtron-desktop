# Repair completion marker must not wait on background installers

- Status: done
- Date: 2026-11-17
- Areas: `crates/steamtron-doctor`

## Problem

The Linux Doctor repair launched the embedded repair script through:

```sh
bash script 2>&1 | tee installer.log
```

`setup.sh install` starts background helpers that inherit the pipeline's stdout.
`tee` then waits for those inherited descriptors, so the wrapper never reached
the `installer.exit` marker even after the visible installer work finished.
SteamTron stayed on “Repairing…” until the 10-minute timeout.

## Goal

The terminal wrapper writes `installer.exit` as soon as the repair script
returns, regardless of background processes started by the installer.

## Non-goals

- Changing the installer itself.
- Making repair ignore installer failures.

## Requirements

- R1: Do not pipe the installer through `tee`; redirect stdout/stderr directly
  to the log.
- R2: Write the exit code marker immediately after the script returns.
- R3: Keep the terminal open long enough to read the result, but exit the
  wrapper after writing the marker.
- R4: `repair_steam_library` still validates the marker and reads the log tail
  on failure.

## Acceptance criteria

- [x] A repair script that starts a background child holding stdout still
  produces `installer.exit`.
- [x] SteamTron leaves the “Repairing…” state after the installer finishes.
- [x] A nonzero installer exit is reported with log output.

## Verification

- Harness command with a background child exited and wrote `installer.exit=0`
  while the child was still running.
- `cargo test -p steamtron-doctor` — 2 passed.
- `cargo check -p steamtron-desktop` — pass.
- 0.1.18 Linux binary built, installed, and SHA-256 matched.
- v0.1.18 release workflow succeeded for Linux and Windows.

## Decisions and risks

- The log is truncated at repair start; a failed run still contains the current
  terminal output.
- Background helpers keep their own descriptors but no longer block the wrapper.
