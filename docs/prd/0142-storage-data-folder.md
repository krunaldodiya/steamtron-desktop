# Storage settings data folder path

- Status: active
- Date: 2026-11-24
- Areas: `steamtron-desktop` settings UI and library snapshot

## Problem

The Storage settings tab showed `C:\Users\<user>\AppData\Local\SteamTron\library.json`
for both **SteamTron data** and **Library file**. Clicking **Open folder** on
the data folder failed with `folder does not exist: ...\library.json`.

`renderStorageSettings` derived the data folder with
`libraryPath.replace(/\/[^/]+$/, "")`. That regex only strips a POSIX
`/filename`; on Windows the backslash path is unchanged, so the file path was
used as the folder.

The same tab counted `0 games` on `D:` even though managed games live under
`D:\SteamTronLibrary\steamtronapps`, because the prefix check assumed forward
slashes and an unprefixed path.

## Goal

The data folder row shows the real SteamTron home directory, the library file
row shows `library.json`, and the open-folder action succeeds. Drive game counts
match managed roots on both Windows and Linux.

## Non-goals

- No change to where the library or appmanifests are stored.
- No change to drive registration.

## Requirements

- R1: `library_json` includes `data_home` from `steamtron_core::paths::layout()`.
- R2: The Storage tab uses `data_home`; the fallback strips either `/` or `\`.
- R3: Drive game counts normalize separators and the Windows `\\?\` prefix.

## Acceptance criteria

- [x] The snapshot exposes `data_home`.
- [x] The UI no longer derives the folder from the file path on Windows.
- [x] Drive counts normalize `\\?\D:\...` and `D:\...`.
- [ ] Runtime: the Storage tab shows the folder, **Open folder** opens it, and
      the `D:` drive count matches the installed games.

## Verification

- `cargo check --workspace` — exit 0 (0.1.63).
- Code inspection of `renderStorageSettings` and `library_json`.
- Runtime verification is pending because 0.1.63 is not built or installed.

## Decisions and risks

- The backend is the source of the data home; the UI fallback only covers an
  older snapshot.
- Drive counting is display-only; it does not affect installation or launch.

## Links

- `docs/prd/0141-appmanifest-settings-config.md`
