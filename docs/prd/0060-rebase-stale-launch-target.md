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

## Verification

- `rebases_an_absolute_target_to_the_managed_root` passes.
- Runtime tests pass: 8.
- The exact GUI Play click was not repeated after this change.

## Decisions and risks

- The rebase is applied at launch; the stored library target is not rewritten.
