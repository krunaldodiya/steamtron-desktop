# Running game state on Windows

- Status: active
- Date: 2026-11-17
- Areas: `crates/steamtron-desktop/src/main.rs`

## Problem

A game is running, but SteamTron still shows Play instead of Stop on Windows.
The play child monitor removed the managed-run marker before waiting for the
child process to exit. Linux masked the bug with `/proc` marker scanning;
Windows had no fallback.

## Fix

Keep the managed-run marker until `child.wait()` returns, then remove it.

## Verification

- Windows VM: start a game, confirm Stop appears for the whole run, stop it, and
  confirm Play returns.
- Linux: existing process-marker behavior remains.
