# Hidden public launch root for managed games (POC)

- Status: active
- Date: 2026-11-26
- Branch: `poc/hidden-public-launch-root` (not merged into `main`)
- Phase: implementation on the experiment branch. The FC5 gate and installed-build
  verification are still pending.
- Areas: Windows overlay layout, library roots, appmanifest, Steam registration,
  backups/restores, security exclusions, Play/Stop lifecycle

## Problem

The current unified layout is:

```text
common\.steamtron-base\<Game>   actual content
common\<Game>                   public junction (idle) / WinFsp mount (playing)
```

This keeps the public install path `common\<Game>` and the actual content
hidden behind `.steamtron-base`. It works, but the user wants the Steam-like
model: actual games directly under `common`, with the junction/mount hidden in
a separate folder.

The earlier `.steamtron-mount` experiment failed because the game was launched
from `common\<Game>` while `GetFinalPathNameByHandle` returned
`.steamtron-mount\<Game>`. The launcher compared those paths and restarted.

A hidden **public launch root** avoids that mismatch:

```text
common\<Game>                    actual content
.steamtron-junction\<Game>       public launch root (junction idle, mount playing)
```

The game is launched from `.steamtron-junction\<Game>` and the final path is
also `.steamtron-junction\<Game>`, so launch path and final path agree.

## Goal

Prove the two-root layout on GTA V Enhanced and Far Cry 5 before refactoring the
whole application:

- actual content directly under `common`;
- hidden public launch root in `.steamtron-junction`;
- overlay mounts at the hidden root with `common\<Game>` as the lower layer;
- the game starts and the hidden junction is restored after Stop;
- actual content is never modified by the overlay.

## Non-goals

- Do not merge the POC into `main` until verified.
- Do not refactor every app consumer in the POC; the POC uses the existing
  wrapper manually.
- Do not change backups, downloads, Verify, DLC or Steam registration in the
  POC.
- Do not test Little Nightmares in this POC; GTA and Far Cry 5 are the gates.
- No per-game code paths; the final feature must be layout-wide.

## Architecture

### Idle

```text
steamtronapps\
├─ common\
│  ├─ Game\                    actual content
├─ .steamtron-junction\
│  ├─ Game                     junction -> common\Game
├─ fixes\
│  └─ Game\
└─ compatdata\
```

### Play

1. Remove `.steamtron-junction\<Game>` (the idle junction).
2. Mount the WinFsp union at `.steamtron-junction\<Game>`:
   - lower: `common\<Game>`;
   - fixes: `fixes\<Game>`;
   - upper: app-data upper.
3. Launch `.steamtron-junction\<Game>\<target>` with cwd under the hidden root.
4. Keep the mount until the whole process tree exits.

### Stop / crash / unmount

1. Unmount the hidden root.
2. Recreate `.steamtron-junction\<Game> -> common\<Game>`.
3. Never write to `common\<Game>`; overlay writes copy up into the upper layer.

## Data model

The final app refactor needs two roots per managed game:

| Field | Purpose | Example |
|---|---|---|
| `content_root` | actual files, downloads, Verify, backups, restores | `...\common\Game` |
| `launch_root` | public path, Play, shortcuts, Steam registration | `...\.steamtron-junction\Game` |

The sidecar becomes:

```json
{
  "content": "...\\common\\Game",
  "launch": "...\\.steamtron-junction\\Game"
}
```

New sidecars store only `content`/`launch`. A downgraded 0.1.77 build cannot
parse that shape, so it rebuilds its own `.steamtron-base` junction layout on
startup instead of misreading the hidden launch path as the game folder.

## Implementation (branch)

The experiment branch implements the two-root model in the desktop app:

- `steamtron-core::library::launch_root(content)` derives
  `common\.steamtron-junction\<Game>` from the content root.
- `RealPathLayout` now stores `content` and `launch`. Existing
  `real`/`base`/`mount` records remain readable as a legacy format.
- Startup migrates every managed game that still has the 0.1.77 junction
  layout:
  1. remove the public junction at `common\<Game>`;
  2. rename `.steamtron-base\<Game>` back to `common\<Game>`;
  3. create `.steamtron-junction\<Game> -> common\<Game>`;
  4. write the new sidecar.
- Migration is a same-volume rename. It never copies content and is retryable
  after an interruption.
- `Play` now mounts at `.steamtron-junction\<Game>` with `common\<Game>` as the
  lower layer. The target executable, arguments and working directory are
  rewritten from the content root to the launch root before the wrapper starts,
  so a content root that is itself a junction still launches from the mount.
- Forced `Stop` restores the launch junction with the existing 20-second retry.
  A dead WinFsp mount that left an empty directory is cleaned up before the
  junction is recreated; a live non-empty mount is never deleted.
- Backups, downloads, Verify and restores keep using `content`; the launch root
  is only used for Play and the overlay.
- The sidecar stores only `content`/`launch`. A downgraded 0.1.77 build sees an
  unknown record and rebuilds its `.steamtron-base` layout automatically.
- Defender exclusions now include both the content folder and the hidden launch
  root, plus a legacy base when one still exists.
- The Fixes UI describes the two-root layout and the button removes only the
  hidden launch root and sidecar; game files stay in the public folder.

Known implementation limits:

- Steam client launches do not pass through the app wrapper; this was already
  true for the 0.1.77 layout. PlayTron Play is the verified overlay path.
- A live game blocks migration/repair of its layout by design.

## Requirements

- R1: Managed game actual content lives directly under `common\<Game>`.
- R2: The hidden public launch root is
      `.steamtron-junction\<Game>` (a junction when idle, a WinFsp mount during
      Play).
- R3: The launch path and the final path are the same hidden root.
- R4: The overlay lower layer is `common\<Game>`; the hidden root is never a
      lower layer.
- R5: Stop recreates the hidden junction to `common\<Game>`.
- R6: A forced Stop / crash must leave `common\<Game>` content intact and allow
      startup repair of the hidden junction.
- R7: Every app consumer must use the correct root:
      - downloads, Verify, DLC, size, backups, restores → `content_root`;
      - Play, Steam registration, shortcuts → `launch_root`.
- R8: Steam/appmanifest `content_directory` and `installdir` semantics must be
      defined for the hidden launch root before the full refactor.
- R9: Defender exclusions cover the hidden launch root, content root and fixes.
- R10: No per-game branches, targets or exceptions.
- R11: The POC must be able to roll back to the current 0.1.77 layout without
      losing content.

## Acceptance criteria

### POC

- [x] PRD committed in the POC branch.
- [x] GTA V Enhanced: content moved to `common\Grand Theft Auto V Enhanced`,
      hidden junction created, wrapper log showed `mounted_at` the hidden root,
      the game window appeared with 2 processes, content hashes were unchanged,
      and the test rolled back to the 0.1.77 layout.
- [ ] Far Cry 5: not run yet in the implementation branch. Pending the branch
      build and install.
- [x] No `.steamtron-mount` directory was created by the GTA POC.
- [x] GTA rolled back to the 0.1.77 layout cleanly.
- [x] `main` remains unchanged.
- [x] Two-root layout implemented in the experiment branch (code-level; not yet
      compiled or installed).

### Full feature (after POC)

- [x] Library/Play/Steam/backups/restores use the correct root (code-level).
- [ ] GTA, FC5, Max Payne, LN2 and a simple game pass (runtime).
- [x] Migration from 0.1.77 is implemented and reversible (code-level).
- [ ] Crash/power-loss repair is verified (runtime).

## POC test procedure

For each of GTA V Enhanced (3240220) and Far Cry 5 (552520):

1. Stop the game and any overlay wrapper.
2. Record content hashes from `.steamtron-base\<Game>`.
3. Remove the public junction `common\<Game>`.
4. Rename `.steamtron-base\<Game>` to `common\<Game>`.
5. Create `.steamtron-junction\<Game> -> common\<Game>`.
6. Run the installed wrapper manually:

   ```text
   --steamtron-overlay --appid <id>
   --common  ...\common\<Game>
   --fixes   ...\fixes\<Game>
   --cwd     ...\.steamtron-junction\<Game>\<target dir>
   --junction ...\.steamtron-junction\<Game>
   --base    ...\common\<Game>
   -- ...\.steamtron-junction\<Game>\<target>
   ```

7. Verify the log reports `mounted_at=...\.steamtron-junction\<Game>` and
   `mount_mode=mount-manager`.
8. Verify the game process and window.
9. Stop the game; wait for unmount; verify the hidden junction points back to
   `common\<Game>` and hashes are unchanged.
10. Roll back:
    - remove the hidden junction;
    - rename `common\<Game>` back to `.steamtron-base\<Game>`;
    - recreate `common\<Game> -> .steamtron-base\<Game>`;
    - remove empty `.steamtron-junction`.

## Verification

GTA V Enhanced POC, executed on 2026-11-26 before the implementation pause:

- Content was moved to `common\Grand Theft Auto V Enhanced`.
- `.steamtron-junction\Grand Theft Auto V Enhanced` was created as the public
  junction.
- Wrapper log:
  - `mounted_at=D:\...\.steamtron-junction\Grand Theft Auto V Enhanced`;
  - `mount_mode=mount-manager`.
- Game process tree: `PLAYGTAV.EXE -> GTA5_ENHANCED.EXE`; window
  `Grand Theft Auto V`; 2 distinct processes, no restart loop.
- Content hashes before/after were identical:
  - `PlayGTAV.exe` `da21b92b…b1dddc`;
  - `GTA5_Enhanced.exe` `b48e80f5…9987ea`.
- `.steamtron-mount` was not created.
- The POC rolled back: `common\Grand Theft Auto V Enhanced` is again a junction
  to `.steamtron-base\Grand Theft Auto V Enhanced`; the hidden root was
  removed.
- Far Cry 5 POC: not run; paused for PRD review.
- No app code was changed; `main` remains 0.1.77.

Implementation branch status (2026-11-26): the two-root code is written and
formatted (`cargo fmt`) but has not been compiled, installed, or run. Runtime
evidence below will be filled only from the rebuilt and installed branch binary.

## Decisions and risks

- The hidden launch root is the key difference from the failed
  `.steamtron-mount` test: launch path and final path are identical.
- `content_directory` and `launcher_relative_path` stay relative to
  `common\<Game>`. They describe the content root and the stored launch target;
  they are not rewritten to the hidden launch root. Play performs the path
  rewrite when it builds the wrapper command.
- Path-sensitive launchers may still validate the hidden path or registry
  install path. GTA/FC5 are the first gates.
- Steam registration may reject a hidden `installdir`; this must be resolved
  before the full refactor, not in the POC.
- Backups/restores must use `content_root`; if they follow `launch_root` while
  a game is mounted, they may capture the overlay view instead of the actual
  content.
- Crash recovery must never leave actual content only in a hidden mount or
  temporary location.
- A forced Stop race is handled by the existing 20-second restore retry; the
  POC must confirm it for the hidden root.

## Rollback

The POC must always be reversible:

```text
.steamtron-junction\<Game>  (remove junction)
common\<Game>              (actual content)
  → rename to .steamtron-base\<Game>
  → recreate common\<Game> -> .steamtron-base\<Game>
  → remove empty .steamtron-junction
```

No content is copied; every operation is a same-volume rename or a junction
change.

## Links

- PRD 0135 (real-path WinFsp overlay)
- PRD 0147 (Mount Manager volume identity)
- PRD 0148 (unified `.steamtron-base` layout)
- PRD 0149 (Play/Stop pending feedback)
- Experiment branch `experiment/hidden-junction-ln1`
