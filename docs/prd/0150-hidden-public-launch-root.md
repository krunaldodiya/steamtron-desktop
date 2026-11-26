# Hidden public launch root for managed games (POC)

- Status: done
- Date: 2026-11-26
- Branch: `poc/hidden-public-launch-root` (merged into `main`)
- Phase: merged. All runtime gates passed (17-game migration,
  GTA/FC5/Max Payne/LN2/LN1 Play/Stop, Little Nightmares
  backup/uninstall/restore, forced-crash repair, running-game safety guard).
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
  layout. The scan runs synchronously in Tauri setup, before Play commands are
  served, so a click cannot race the migration:
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
- [x] Far Cry 5: branch build installed. Play from the app mounted at
      `.steamtron-junction\Far Cry 5` with `mount_mode=mount-manager` and
      started `FARCRY5.EXE` from the hidden path; the user confirmed the game
      was running. Stop restored the launch junction. Executable hash was
      unchanged.
- [x] No `.steamtron-mount` directory was created by the GTA POC or the branch
      build.
- [x] GTA rolled back to the 0.1.77 layout cleanly.
- [x] `main` remains unchanged.
- [x] Two-root layout implemented, compiled, and installed in the experiment
      branch.
- [x] Startup migration converted all 17 managed games with 0 failures:
      real content under `common\<Game>`, launch junction under
      `.steamtron-junction\<Game>`, sidecar `content`/`launch`.
- [x] Backup/uninstall/restore round trip verified on Little Nightmares:
      app uninstall removed content, junction and sidecar; restore from the
      existing `G:` backup recreated content, junction, sidecar and library
      target with the original executable hash.

### Full feature (after POC)

- [x] Library/Play/Steam/backups/restores use the correct root (code-level).
- [x] GTA, FC5, Max Payne, LN2 and a simple game pass (runtime). All five
      launched from the hidden root and returned to the content junction.
- [x] Migration from 0.1.77 is implemented and reversible.
- [x] Crash/power-loss repair is verified (runtime). Forced-crash repair is
       verified: after killing the wrapper and game during Play,
       `common\Little Nightmares` stayed intact and restarting the app
       recreated `.steamtron-junction\Little Nightmares`. Actual power-loss
       was not simulated.
- [x] Backup creation from the new layout and restore over an existing install
      both pass on Little Nightmares.

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

Runtime test build (`d869e85`), 2026-11-26:

- Windows CI run `45791546643` passed; installer
  `SteamTron_0.1.77_x64-setup.exe` SHA-256
  `31e58d08c0dde0d67d7021a96288124976335ac778d2143f8a4774d91bf66e72`.
- Installed `steamtron-desktop.exe` SHA-256
  `b709c28c8521e0d8b363c384ef97907b4268f552bc612503ae45bd61766a28b2`,
  version 0.1.77.
- Startup `overlay-layout.log`: `migrated=17 failed=0 managed=17`.
- Layout verifier: 17/17 PASS; every `common\<Game>` is a real directory,
  every `.steamtron-junction\<Game>` is a junction to it, every sidecar has
  `content`/`launch`, legacy `.steamtron-base` folders are gone, and
  `.steamtron-mount` does not exist.

Final pre-merge build (`f9cb5cd`, Windows CI run `45802074030`):

- Installer SHA-256
  `c162c62557c6beb4329911bc0d5db62cdd7158cd4c881c4be306d2cedf529f7d`;
  installed `steamtron-desktop.exe` SHA-256
  `05a335d94c4f6cdd6fc4f9272ea663b5b198dd84705f779422bf4f8974d04e12`.
- Restart on the already-migrated layout was idempotent: no new migration,
  layout verifier 17/17 PASS.
- Running-game safety guard smoke test: with Little Nightmares playing from
  the hidden mount, a restore was attempted. The app refused it with
  `Stop the game before restoring its backup.`; the game kept running and its
  executable hash stayed `02a06e91…f1485d`. Stop then restored the junction.

GTA V Enhanced (3240220), app Play:

- `PLAYGTAV.EXE` and `GTA5_ENHANCED.EXE` command lines both used
  `common\.steamtron-junction\Grand Theft Auto V Enhanced`.
- Play log: `mounted_at` the hidden root, `mount_mode=mount-manager`.
- The user confirmed the game was running.
- `PlayGTAV.exe` SHA-256 stayed `da21b92b…b1dddc`; `GTA5_Enhanced.exe`
  stayed `b48e80f5…9987ea`.

Far Cry 5 (552520), app Play/Stop:

- Play log:
  - `mounted_at=D:\...\.steamtron-junction\Far Cry 5`;
  - `mount_mode=mount-manager`;
  - `exec=D:\...\.steamtron-junction\Far Cry 5\bin\FarCry5.exe`.
- `FARCRY5.EXE` command line was the hidden launch path; the user confirmed
  the game was running.
- Stop through the app returned the button to Play and restored
  `.steamtron-junction\Far Cry 5 -> common\Far Cry 5`.
- `FarCry5.exe` SHA-256 stayed
  `e12e0a0dfb7d1bb48da2fee78aa172288c5b2495b09dffec9426663c6a028c73`.
- A manual wrapper run also started `FARCRY5.EXE` from the hidden path with an
  unchanged hash. That harness killed the wrapper while child processes were
  still alive, so its launch junction was not restored; the app Stop path
  restored it correctly.

Little Nightmares (424840), backup and restore round trip:

- Existing `G:\Games\SteamTronGames\Little Nightmares` backup validated:
  `content.tar` 9,770,853,888 bytes, SHA-256
  `e435122d50a4237e8b9339fb80734f62b95ed1655b4cf5e13f9a4204e404d49a`
  (matches its manifest); fixes TAR checksum matches; the executable inside
  the backup is `02a06e91…f1485d`.
- App Uninstall removed `common\Little Nightmares`, the hidden launch junction
  and the overlay sidecar. The library entry stayed with `confirmed=false` and
  no launch targets; fixes and prefix were kept.
- Restore from that backup recreated the content, the hidden launch junction,
  the sidecar and the original library target. The executable SHA-256 was
  again `02a06e91…f1485d`; no legacy `.steamtron-base` was created.
- A fresh backup written from the new layout to
  `G:\Games\SteamTronGames\RoundTrip\Little Nightmares`:
  - `content.tar` 9,770,853,888 bytes;
  - actual SHA-256 `57974a06…fcd907` matches the backup manifest;
  - fixes TAR checksum matches;
  - the source executable stayed `02a06e91…f1485d`.
- Restoring that fresh backup over the installed game also passed: content,
  launch junction, sidecar, library target and executable hash were restored.
- Little Nightmares app Play mounted at the hidden root and executed
  `LITTLENIGHTMARES.EXE` from it; app Stop restored the junction. This run is
  not counted as a game-window proof because the automation stopped it while
  the Play confirmation dialog was still being handled.

Max Payne 3 (204100), app Play/Stop:

- `PLAYMAXPAYNE3.EXE` ran from
  `common\.steamtron-junction\Max Payne 3\Max Payne 3\PlayMaxPayne3.exe`.
- Play log: hidden `mounted_at`, `mount_mode=mount-manager`.
- Stop used the app confirmation; the launch junction returned to
  `common\Max Payne 3`.
- `PlayMaxPayne3.exe` SHA-256 stayed `f0ac2ac1…0a09762`.

Little Nightmares II (860510), user-driven app Play/Stop:

- User confirmed the game started and stopped successfully.
- Fresh log: `mounted_at` the hidden root, `mount_mode=mount-manager`,
  `exec` the hidden `Little Nightmares II.exe`.
- Launch junction restored to `common\Little Nightmares II`; sidecar still
  `content`/`launch`; no `stop-restore.log`.
- `Little Nightmares II.exe` SHA-256 stayed `2920c170…bd1c9`.

Little Nightmares (424840), app Play/Stop after the restore:

- Play confirmation accepted; `LITTLENIGHTMARES.EXE` ran from the hidden path.
- Play log: hidden `mounted_at`, `mount_mode=mount-manager`.
- Stop confirmation accepted; launch junction restored to
  `common\Little Nightmares`.
- Executable SHA-256 stayed `02a06e91…f1485d`.

Crash recovery (Little Nightmares):

- Play mounted the hidden root and the user-visible game process started.
- The wrapper, game process and app were force-killed without Stop; the
  launch path disappeared, but `common\Little Nightmares` remained and its
  executable hash stayed `02a06e91…f1485d`.
- Restarting the app recreated
  `.steamtron-junction\Little Nightmares -> common\Little Nightmares`; the
  sidecar stayed `content`/`launch`, the library stayed installed, and the
  hash was still unchanged.
- Actual power loss was not simulated.

Remaining runtime gates: none for the POC.

Safety guards from commit `159e641` are included in the final `f9cb5cd` build
and were smoke-tested with a restore attempt while Little Nightmares was
running.


## Decisions and risks

- The hidden launch root is the key difference from the failed
  `.steamtron-mount` test: launch path and final path are identical.
- `content_directory` and `launcher_relative_path` stay relative to
  `common\<Game>`. They describe the content root and the stored launch target;
  they are not rewritten to the hidden launch root. Play performs the path
  rewrite when it builds the wrapper command.
- Path-sensitive launchers may still validate the hidden path or registry
  install path. GTA/FC5 are the first gates.
- Steam registration publishes only the App ID/Lua; `content_directory` remains
  `common\<Game>` and no hidden `installdir` is written. Steam-client launches
  bypass the app overlay, the same as the 0.1.77 layout.
- Backups and restores use `content_root`, not `launch_root`. Verified by the
  Little Nightmares uninstall/restore and backup-over-existing round trips.
- Crash recovery must never leave actual content only in a hidden mount or
  temporary location.
- A forced Stop race is handled by the existing 20-second restore retry; FC5
  and Little Nightmares app Stop both restored the launch junction.

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
