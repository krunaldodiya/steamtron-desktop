# Rebase stale launch targets

- Status: done
- Date: 2026-11-16
- Areas: `crates/steamtron-core/src/runtime.rs`

## Problem

After a DLC update, Play failed with:

```text
launch target does not exist:
/mnt/krunal/SteamLibrary/steamapps/common/Little Nightmares/Atlas/Binaries/Win64/LittleNightmares.exe
```

The library entry pointed at an old Steam library path, while the managed files
live under `SteamTronLibrary/steamtronapps/common/Little Nightmares`. DLC was
not the cause; it exposed the stale target when Play was next used.

## Requirements

- R1: Prefer an existing target path unchanged.
- R2: Rebase an absolute stale target by finding the game-root component and
  appending the suffix to the current managed root.
- R3: Fall back to the executable filename inside the managed root.
- R4: The overlay launch cwd must come from the resolved executable, not the
  stored target, so a rebased exe does not get a stale working directory.

## Verification

- `rebases_an_absolute_target_to_the_managed_root` passes.
- Runtime tests pass: 8.
- 0.1.68: Little Nightmares and II still failed because the cwd used the
  stale stored target (`D:\mnt\krunal\...`) even though the exe was rebased to
  `S:`. After regenerating the manifests and fixing cwd to the resolved
  executable directory, Little Nightmares launched (PID 8736) with
  `cwd=S:\Atlas\Binaries\Win64` in the overlay log.
- 0.1.69 includes the cwd fix; all 17 installed-game manifests were
  regenerated from the live library (2 stale targets rebased).

## Decisions and risks

- The rebase is applied at launch; the stored library target is not rewritten.
