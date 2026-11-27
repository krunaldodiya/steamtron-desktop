# Linux hidden launch root for managed games

- Status: draft
- Date: 2026-11-27
- Branch: `poc/linux-hidden-launch-root` (not merged)
- Areas: Linux overlay backend, library roots, Play/Stop lifecycle, backups/restores

## Problem

Windows now uses a stable two-root layout:

```text
common/<Game>                         real content
common/.steamtron-junction/<Game>     idle junction; WinFsp mount during Play
```

Linux still uses the old strategy. `LinuxOverlay` mounts `fuse-overlayfs` at a
session-specific runtime path:

```text
/tmp/steamtron-<uid>/<game>-<session>/merged
```

That path changes on every Play. It is the Linux equivalent of the old
`S:\` drive-letter mount, and it is even less stable because the path includes
a new session ID. Games that do not care about their install path work; games
that validate it (GTA V Enhanced, Far Cry 5 and other launchers) fail or
restart, matching the Windows failures seen before the two-root layout.

## Goal

Give Linux the same stable path model:

```text
common/<Game>                         real content
common/.steamtron-junction/<Game>     idle symlink; fuse-overlayfs mount during Play
```

Play launches Proton/game against `.steamtron-junction/<Game>`, and the final
path is the same path. Stop and crash recovery restore the idle symlink.
Backups, restores, downloads and Verify continue to use `common/<Game>`.

## Non-goals

- Do not change Windows behavior; all Windows tests must keep passing.
- Do not add per-game Linux exceptions or paths.
- Do not make Steam-client launches use the overlay; that bypass already
  exists on both platforms.
- Do not treat WSL or WSLg as a game/Proton verification environment.
- Do not require games to live on an NTFS mount.

## Requirements

- R1: Managed content stays in `common/<Game>`.
- R2: The Linux launch root is
      `common/.steamtron-junction/<Game>`.
- R3: While idle, the launch root is a symlink to `common/<Game>`.
- R4: During Play, the symlink is removed and `fuse-overlayfs` mounts at the
      launch root:
      - lower: `fixes/<Game>` over `common/<Game>`;
      - upper/work: SteamTron app-data;
      - the launch root is the mount point.
- R5: Proton, the executable, arguments and working directory all use the
      launch root; the final path equals the launch path.
- R6: Stop unmounts and recreates the symlink with retry while fuse-overlayfs
      releases the mount.
- R7: A killed wrapper or app crash must leave `common/<Game>` intact and let
      startup repair recreate the symlink.
- R8: The sidecar stores `content`/`launch` and is shared with Windows.
- R9: Backup, restore, Verify and size use the content root. Restore removes
      the idle symlink before replacing content and recreates it afterward.
- R10: Move and Uninstall refuse while the game is running, and remove/recreate
      the launch root safely.
- R11: The Fixes UI reports the Linux layout and offers "Remove overlay layout"
      (content stays in place).
- R12: No per-game branches, paths or exceptions.
- R13: Windows CI and the existing Windows 17-game layout remain unchanged.

## Acceptance criteria

### WSL / CI mechanics

- [ ] `cargo check --workspace` and `cargo test -p steamtron-core` pass.
- [ ] A dummy managed game starts with:
      `common/<Game>` real content,
      `.steamtron-junction/<Game>` symlink while idle.
- [ ] Play removes the symlink, mounts fuse-overlayfs at the hidden root,
      exposes a fixes marker over the content, and launches a process whose
      argv[0] and cwd are under the hidden root.
- [ ] A write during Play copies up to the upper layer and leaves
      `common/<Game>` unchanged.
- [ ] Stop unmounts and recreates the symlink pointing to `common/<Game>`.
- [ ] Forcibly killing the wrapper/mount leaves content intact and a simulated
      startup repair recreates the symlink.
- [ ] A backup reads `common/<Game>`, and a restore replaces content then
      recreates the symlink.
- [ ] Windows CI still builds and the Windows 17/17 layout verifier passes.

### Real Linux gate (requires a Linux desktop)

- [ ] Install the Linux package on a real Linux system with Steam/Proton.
- [ ] GTA V Enhanced starts from `.steamtron-junction/<Game>` and shows its
      game window.
- [ ] Far Cry 5 starts from `.steamtron-junction/<Game>` including its
      launcher/elevation path.
- [ ] Stop restores the symlink; content hashes are unchanged.
- [ ] At least one simple managed game passes Play/Stop.

## Verification

Pending implementation. WSL can verify the FUSE/symlink lifecycle with a dummy
game but cannot verify Steam/Proton, GPU, or launcher path validation.

### WSLg mechanics test

WSLg can run the Linux Tauri app and is useful for:

- starting the app against a temporary WSL library root;
- checking the Linux layout status UI and "Remove overlay layout";
- creating/removing the idle symlink;
- running the overlay wrapper manually against a dummy executable or shell
  script, without Steam or Proton;
- verifying the fixes overlay, copy-up, hidden argv[0]/cwd, Stop/crash restore,
  and backup/restore directory handling.

WSLg is not a game gate:

- Proton/DXVK/VKD3D need a reliable Vulkan driver; WSLg's GPU stack is not a
  substitute for a native Linux desktop.
- GTA V and Far Cry 5 also depend on their launchers, DRM, input, audio and
  anti-cheat behavior.
- A Steam/Proton install inside WSL is not representative of the Linux target.

The real Linux gate still requires a Linux desktop with Steam and Proton.


## Decisions and risks

- Linux uses a symlink for the idle launch root because it has no junction
  reparse points. The path is still stable across runs, which is the important
  property for launchers.
- fuse-overlayfs will be asked to mount at the hidden path after removing the
  symlink. The mount point must not be a symlink at mount time.
- Existing Proton prefixes may contain registry paths from the old temporary
  merged path. The first two-root Play may need prefix repair; this is a
  runtime gate, not something WSL can prove.
- Proton and some launchers resolve symlinks. That is acceptable while idle;
  during Play the path is a real mount, matching the Windows behavior.
- Games on NTFS mounts are not a valid Linux gate because Proton, permissions
  and case behavior differ. Test on native Linux storage.
- WSL provides `/dev/fuse` and `fusermount3`, so the overlay mechanics and a
  dummy executable can be tested here. It is not evidence that GTA/FC5 work.
- SteamTron's Linux package will be built in CI and installed on a real Linux
  machine for the game gates.

## Links

- PRD 0147 (Windows Mount Manager identity)
- PRD 0148 (Windows unified base layout)
- PRD 0150 (Windows hidden public launch root)
- `crates/steamtron-overlay/src/linux.rs`
- `crates/steamtron-desktop/src/main.rs`
