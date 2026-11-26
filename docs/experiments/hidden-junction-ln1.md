# Hidden `.steamtron-junction` launch experiment

- Branch: `experiment/hidden-junction-ln1` (not merged into `main`)
- Date: 2026-11-26
- Game: Little Nightmares (424840)
- Script: `scripts/experiments/hidden-junction-ln1.ps1`

## Goal

Check whether the public real-path junction can live in a hidden
`common\.steamtron-junction\<Game>` folder instead of `common\<Game>`, while
the actual content stays in `common\.steamtron-base\<Game>`.

## Setup

```text
common\.steamtron-base\Little Nightmares\        actual content
common\.steamtron-junction\Little Nightmares     junction -> base
```

The overlay wrapper was invoked manually with:

```text
--common      ...\.steamtron-base\Little Nightmares
--junction    ...\.steamtron-junction\Little Nightmares
--base        ...\.steamtron-base\Little Nightmares
--            ...\.steamtron-junction\Little Nightmares\Atlas\Binaries\Win64\LittleNightmares.exe
```

## Result

- Mount Manager mount succeeded:
  `mounted_at=D:\...\common\.steamtron-junction\Little Nightmares`,
  `mount_mode=mount-manager`.
- `LITTLENIGHTMARES.EXE` started from the hidden path.
- Base executable SHA-256 was unchanged before/after:
  `02a06e911a9d4d9a0ac4dfb39e2219bb23ed9e0d436e5ec3fd1af38e32f1485d`.
- After the test game was stopped, the wrapper unmounted the hidden path and
  the hidden junction was **not** recreated automatically; the hidden path was
  missing afterward.
- The app, Steam registration and `appmanifest_424840.json` still point at
  `common\Little Nightmares`, not the hidden path.

## Conclusion

The overlay can mount and run a simple game from `.steamtron-junction`, but
that does not make it the public install path. The public path used by Steam,
the appmanifest, backups and launchers remains `common\<Game>`. For
path-sensitive launchers, a hidden path would validate incorrectly.

This is therefore not a drop-in folder rename and not a viable app-wide
structure without changing every consumer of the game path. The permanent
public junction/mount must stay at `common\<Game>`.

The branch is left as an experiment only; `main` is unchanged.
