# Windows verbatim path separator normalization

- Status: active
- Date: 2026-11-24
- Areas: `steamtron-core` library path overlay, `steamtron-desktop` real-path helpers

## Problem

A managed game whose `installation.root` uses the Windows `\\?\` prefix and
whose appmanifest has a `content_directory` with `/` was built as:

```text
\\?\D:\SteamTronLibrary\steamtronapps\common/Max Payne 3
```

Win32 does not normalize a verbatim path. A `/` in that string is a name
character, not a separator, so `GetFileAttributesW` returns
`ERROR_INVALID_NAME` (123). Rust also treats the whole `common/Max Payne 3`
as one component.

Symptoms observed for Max Payne 3:

- Play fails with `game content folder does not exist`.
- Installed Files shows `0 B` while the folder contains 34.9 GB.
- The pre-plan repair reports `(base folder is missing)` because it derives
  parent `...\steamtronapps` and name `common/Max Payne 3`.
- The fixes overlay works when given the plain base path; the junction and
  spaces are valid.

The malformed root was produced by `rebase_with_prefix` in
`steamtron-core/src/library.rs`, which prepended `\\?\` to a candidate that
still contained `/`. It affects every managed game with a verbatim root, not
just Max Payne 3.

## Goal

Any Windows path that begins with `\\?\` contains only `\` separators. The
app reads and launches the existing game folder without changing the
on-disk layout.

## Non-goals

- No per-game workaround.
- No removal of the `\\?\` long-path design.
- No UNC support expansion (handled defensively but not a supported target).

## Requirements

- R1: Normalize `/` to `\` before adding or keeping a `\\?\` prefix.
- R2: `Game::apply_appmanifest` normalizes joined launcher executables.
- R3: `repair_game_root` normalizes the root before computing its parent,
  name and `.steamtron-base` sibling.
- R4: In-memory game paths are normalized before real-path preparation and
  launch planning.
- R5: Path comparisons treat `\\?\D:\...` and `D:\...` as the same location.
- R6: `Game::appmanifest` never writes a folder component containing `/`
  (prevents `common/common/<Game>` corruption).

## Acceptance criteria

- [x] A separator-normalization unit test covers a mixed path.
- [x] `cargo check --workspace` passes.
- [ ] Runtime: Max Payne 3 Play resolves `PlayMaxPayne3.exe`.
- [ ] Runtime: Installed Files reports the 34.9 GB game size.
- [ ] Runtime: a settings save keeps `content_directory` as
      `common/Max Payne 3`.

## Verification

- Root cause reproduced read-only: `GetFileAttributesW` on the mixed verbatim
  path returns `ERROR_INVALID_NAME`; the all-backslash path and the junction
  are valid.
- `cargo test -p steamtron-core --lib -- --skip terminates_a_process_group`
  and `cargo test -p steamtron-library --lib` pass.
- Runtime verification is pending until 0.1.67 is installed.

## Decisions and risks

- Normalization happens at the boundary where a plain path becomes verbatim;
  the long-path prefix remains for valid paths.
- A legacy mixed path is normalized in memory before launch, so existing
  `library.json`/manifest data does not need a manual repair.
- The primary fix is general; no game-specific target or catalog entry is
  added.

## Links

- `docs/prd/0137-app-wide-real-path-manager.md`
- `docs/prd/0141-appmanifest-settings-config.md`
