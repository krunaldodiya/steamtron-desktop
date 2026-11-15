# Downloads continue after SteamTron exits

- Status: active
- Date: 2026-11-15
- Areas: `crates/steamtron-desktop/src/download.rs`,
  `crates/steamtron-desktop/src/main.rs`,
  `crates/steamtron-desktop/ui/app.js`

## Problem

The SMD downloader was started with piped stdout/stderr. When SteamTron exited,
the pipe readers disappeared, so SMD could fail with a broken pipe and the app
had no way to reconnect. A download could not continue while the app was closed,
and reopening SteamTron showed no transfer even if SMD was still running.

## Goal

SMD owns the transfer independently of the window. It survives a full SteamTron
exit, keeps writing progress to disk, and the next SteamTron launch reconnects to
the same PID and log. The manager shows the restored transfer and can pause,
resume, or cancel it. If SMD finished while the app was closed, the next launch
finalizes the install and marks the game playable.

## Non-goals

- Surviving a machine reboot.
- A separate systemd service or OS-level download daemon.
- Finalizing a download with SteamTron never reopened; SMD continues writing,
  and adoption happens on the next launch.
- Changing SMD's own chunk-resume behavior.

## Requirements

- R1: SMD stdout is redirected to a persistent JSON log; stderr goes to a
  separate file so it cannot corrupt JSON lines.
- R2: SMD runs in its own process group and is not killed when SteamTron exits.
- R3: A per-game sidecar (`active-download.json`) records game ID, PID, log, and
  game root.
- R4: Next launch scans sidecars, restores live state, and tails the existing log
  from the beginning to rebuild progress and depot rows.
- R5: A restored process that is SIGSTOP-paused is reported as paused and can be
  resumed.
- R6: A completed log from a closed session is finalized on next launch: download
  directory adopted, launch target detected, library entry marked installed.
- R7: An interrupted download keeps partial files so Download resumes instead of
  deleting them; an explicit cancel still removes them.
- R8: The sidecar is removed after finalization.

## Acceptance criteria

- [ ] Start a download, fully quit SteamTron, and verify SMD remains alive.
- [ ] Reopen SteamTron and see the same transfer with progress and depot rows.
- [ ] Pause, resume, and cancel work after reconnecting.
- [ ] A transfer completed while closed is installed on next launch.
- [ ] An interrupted transfer leaves partial files and Download resumes it.
- [ ] Cancel still removes partial content.

## Verification

- Started a managed download for App 113200 in the installed build.
- Sent SIGTERM to the SteamTron process. `ps` showed SMD still alive with PPID
  1 and established Steam CDN sockets, confirming it outlived the app.
- The per-game sidecar contained the live PID, JSON log, and game root.
- Relaunched SteamTron; the game detail restored to "Downloading", the manager
  reopened, and depot state was rebuilt from the persisted log.
- Full completion-while-closed adoption and post-restart pause/cancel were not
  exercised in this run; they use the same monitor/finalize path as live
  downloads.

## Decisions and risks

- The desktop binary remains the only program; no second supervisor binary is
  introduced. SMD is already a separate process, so redirecting its output is
  enough for it to outlive the UI.
- Restore replays the log rather than persisting a serialized UI state. The log
  is the source of truth and also captures completion and depot events.
- A partial final JSON line is buffered until its newline arrives.
- Pause/resume still rely on Unix signals from the recorded PID.
- Finalization is deferred if SteamTron is not running when SMD completes.

## Links

- Related PRDs: `docs/prd/0024-download-manager.md`,
  `docs/prd/0022-smd-downloader.md`
- Implementation: `crates/steamtron-desktop/src/download.rs`
