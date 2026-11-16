# Reliable Steam installation detection

- Status: done
- Date: 2026-11-16
- Areas: `crates/steamtron-core/src/{steam,runtime}.rs`

## Problem

`steam_root()` treated any directory containing `steamapps` as a Steam
installation. Selecting a Steam prefix in game settings created
`~/.local/share/Steam/steamapps/compatdata/<appid>/pfx`, so the folder alone
made an uninstalled Steam look installed.

## Requirements

- R1: A candidate root must contain a real Steam marker, not only `steamapps`.
- R2: Linux markers include `steam.sh`, `linux32/steam`, `ubuntu12_32/steam`,
  `registry.vdf`, `config/config.vdf`, `config/loginusers.vdf`, or
  `steamapps/libraryfolders.vdf`.
- R3: Windows markers include `steam.exe`, `registry.vdf`,
  `config/loginusers.vdf`, or `steamapps/libraryfolders.vdf`.
- R4: Flatpak Steam at
  `~/.var/app/com.valvesoftware.Steam/.local/share/Steam` is discovered.
- R5: A Steam-mode prefix launch fails with `Steam installation` before
  creating any prefix directory when Steam is not installed.
- R6: Prefix selection already uses `steam_root()` and therefore rejects Steam
  mode with no installation.

## Verification

- Unit tests:
  - `compatdata_only_root_is_not_a_steam_install` passes.
  - `steam_marker_marks_a_real_install` passes.
- On this machine, where only the fake
  `~/.local/share/Steam/steamapps/compatdata/1817190/pfx` tree exists,
  `steam_root()` now returns `Steam installation was not found`.

## Decisions and risks

- The stale fake folder is not deleted automatically: its compatdata prefix can
  contain saved game state. Detection ignores it, and no new prefix is created
  while Steam is absent.
- A package-installed Steam that has never run has no user root yet, so it is
  reported as not installed rather than guessed.
