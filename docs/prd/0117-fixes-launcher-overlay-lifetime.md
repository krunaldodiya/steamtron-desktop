# Fixes-folder launchers and overlay lifetime

- Status: active
- Date: 2026-11-19
- Areas: `crates/steamtron-overlay/src/lib.rs`,
  `crates/steamtron-core/src/paths.rs`, `crates/steamtron-library/src/managed.rs`,
  `crates/steamtron-desktop/src/main.rs`,
  `crates/steamtron-desktop/src/process_tree.rs`,
  `crates/steamtron-desktop/ui/app.js`

## Problem

Some fixes packs ship their own launcher. Red Dead Redemption 2's Razor1911
pack delivers `Launcher.exe` in
`D:\SteamTronLibrary\steamtronapps\fixes\Red Dead Redemption 2`, but SteamTron
would not accept it:

- `validate_launch_executable` and `pick_launch_executable` rejected every
  executable outside the Steam install (`game.common`), so the fixes launcher
  could never be stored as a launch target.
- `link_managed_game` required launchers inside the linked folder, so the
  Game Link tab could not add it either.
- `OverlayGame::rewrite_arg` only mapped the game root onto the merged drive.
  A fixes launcher path was therefore passed through unchanged, outside the
  union that contains the game and the other fix files.

Even with the launcher copied into the game folder by hand, Play exited almost
immediately: in the 16:35:11 run `Launcher.exe` exited `0`, `system.xml` was
written at 16:36:18, `exit_file.dat` reported `[EXIT_CODE] 0x00000000` at
16:36:32, and no profile was created. The wrapper waited only for the direct
child, so it unmounted the union as soon as the launcher exited.

An instrumented run of the installed 0.1.37 binary on 2026-11-19 at 16:58:13
showed RDR2's launcher stays alive for the whole session, so that unmount is
not what ended the earlier session: `S:\Launcher.exe` (pid 22484) started
`S:\RDR2.exe` (pid 8684) three seconds later, `S:\` stayed mounted, and the
game ran until it was stopped manually at 17:02:30 with no crash and no WER
entry. The system log at 16:33 instead shows missing DLL popups for
`ffx_fsr2_api_x64.dll`, `oo2core_5_win64.dll`, `amd_ags_x64.dll`, and
`NvLowLatencyVk.dll`, consistent with the old library root
`D:\SteamLibrary\steamapps\common\Red Dead Redemption 2` being moved while the
game was running. The descendant wait in R5 therefore stays as a safety net
for launchers that detach and exit, not as the explanation for the RDR2 crash.

## Goal

A launcher that lives in a game's fixes folder can be selected from both the
Game Link tab and the Fixes tab, starts from the merged overlay view on Windows
and Linux, and the overlay stays mounted until every process the launcher
started has exited.

## Non-goals

- Running a fixes launcher without the overlay.
- Changing which target Play picks when several are configured.
- Detecting or repairing a game's own configuration/profile problems.

## Requirements

- R1: `OverlayGame::rewrite_arg` maps absolute paths under the game root **or**
  the fixes root onto the merged root, comparing without the Windows verbatim
  prefix and case-insensitively on Windows; sibling folders are not matched.
- R2: `validate_launch_executable`, `add_launch_target`, and the Game Link
  `link_game` flow accept an executable inside the game folder or the game's
  fixes folder, and reject anything else.
- R3: Both UIs offer a fixes-folder picker (`pick_launch_executable` and
  `pick_managed_executable` gain `source: "fixes"`), so the fixes path is
  stored without the user navigating there manually.
- R4: Selecting a fixes launcher while no overlay is active fails with an
  explanatory error instead of running the bare fixes executable.
- R5: The wrapper keeps the overlay mounted while any descendant process of
  the launcher is alive (Windows job object with
  `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`; Linux `/proc` scan for `exe`/`cwd`
  inside the merged root) and only then unmounts.

## Acceptance criteria

- [x] Unit tests prove the fixes-root rewrite, verbatim-prefix handling, and
      that sibling fixes folders are left untouched; a Unix variant covers
      `/mnt/...` roots.
- [x] A fixes-folder `Launcher.exe` launches from `S:\` with the game content
      visible (instrumented wrapper run on the installed build). The same path
      through a *stored* Play target was not exercised because the native file
      dialog cannot be driven here.
- [x] RDR2 started through the fixes `Launcher.exe` stayed up for 3m41s from
      `S:\RDR2.exe` with no crash or WER entry; stopping the wrapper killed the
      launcher and game and unmounted `S:\`.
- [ ] Storing the fixes launcher with the new picker in the Fixes and Game Link
      tabs, then Play from that stored target (needs a real click on the native
      dialog).
- [ ] Removing the overlay while a fixes launcher is selected reports the
      explanatory error (code inspected, not exercised).

## Verification

- Install 0.1.37 runtime proof (before this change): instrumented wrapper with
  `-- <common>\Launcher.exe` logged `merged=S:\`, `cwd=S:`,
  `exec=S:\Launcher.exe`; `LAUNCHER.EXE` (pid 22484) started `RDR2.EXE`
  (pid 8684) from `S:\RDR2.exe`; the game ran from 16:58:13 until 17:02:30 with
  no crash, and `S:` disappeared after the wrapper was stopped.
- Code-level: `cargo check -p steamtron-desktop` finished with no errors
  (pre-existing warnings only). `cargo test -p steamtron-overlay rewrite`:
  3 passed, including `rewrite_arg_maps_fixes_launchers_to_the_merged_drive`.
  `cargo test -p steamtron-core paths::`: 3 passed, including
  `path_within_accepts_roots_and_descendants_only`.
  `cargo test -p steamtron-library managed::tests`: the new
  `links_a_launcher_from_the_fixes_folder` and
  `rejects_a_launcher_outside_the_game_and_fixes_folders` pass (10 of 11).
  `adds_a_managed_game_and_registers_it` fails on Windows at
  `report.root.ends_with("SteamTronLibrary/steamtronapps/common/Sample Game")`
  because `String::ends_with` compares the literal `/` against a
  `\`-separated path; that assertion is unchanged from HEAD (line 1175) and
  unrelated to this change.
- Release build: `cargo build --release -p steamtron-desktop` finished; SHA-256
  `7768CF3EA0F2904403B6AA923022B516AA3296B955438D069F9310C95332DD02`, installed
  to `%LOCALAPPDATA%\Programs\SteamTron\steamtron-desktop.exe` (same hash,
  written 17:13:23).
- Fixes-folder run on that installed build (17:14:40): wrapper received
  `-- D:\SteamTronLibrary\steamtronapps\fixes\Red Dead Redemption 2\Launcher.exe`
  and logged `merged=S:\`, `cwd=S:`, `exec=S:\Launcher.exe`. `LAUNCHER.EXE`
  (pid 18700) started `RDR2.EXE` (pid 24792) from `S:\RDR2.exe`; both were
  still running at 17:18:21 when the wrapper was force-stopped. Afterwards
  neither process nor the `S:` drive existed, no Application Error/WER event
  was logged in the last 10 minutes, and the job object's
  `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` removed the tree.
- Not verified: the two UI clicks above. The dialogs use `rfd`/OS pickers, so
  the picker and the no-overlay error message were not exercised; only their
  code paths and the backend validation tests were inspected.

## Decisions and risks

- A job object is used on Windows because a launcher may spawn a detached game
  and exit; the job follows the tree and is killed if the wrapper dies, which
  matches the Stop button's previous behaviour.
- Linux has no job object; the wrapper polls `/proc` for processes whose
  `exe` or `cwd` is inside the merged root. This misses a process that has
  already `chdir`ed away and re-executed from elsewhere; accepted because the
  alternative is a cgroup or pidfd supervisor.
- Risk: an unrelated process inside the merged root keeps the overlay mounted
  after the game exits; the merged root is per-launch, so this is unlikely.
- The copied `Launcher.exe` in the game folder is user work; it is left in
  place and only the stored target changes.

## Links

- Builds on `0096-windows-union-overlay.md`,
  `0113-linux-overlay-launch-env.md`, and
  `0116-linked-fixes-managed-location.md`.
