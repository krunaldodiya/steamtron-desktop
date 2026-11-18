# Windows must not publish depot manifests to Steam

- Status: done
- Date: 2026-11-18
- Areas: `crates/steamtron-register/src/windows.rs`

## Problem

Every time Steam started it queued an update for **Steamworks Common
Redistributables (228980)** and began downloading ~500 MB, even though SteamTron
had already downloaded the games into `SteamTronLibrary`.

`content_log.txt` showed:

```
AppID 3468650 scheduler update : Priority Auto Update ...
AppID 3468650 state changed : Update Required,Update Queued,
AppID 228980 ... BytesToDownload 497072672
```

The Windows registration adapter wrote each game's depot `.manifest` files into
`Steam/steamapps/depotcache`. Steam interprets those as installed depots and
schedules updates for the app and its dependencies.

## Goal

Windows registration grants the Lua licenses and writes stplug-in manifests
only; Steam never receives depot manifests it did not install, so no automatic
downloads start.

## Requirements

- R1: `BtsBackend::register` does not write `steamapps/depotcache` files.
- R2: It removes an existing file with the same name for that registration, so
  already-registered games are cleaned on the next sync.
- R3: The returned `Registration.manifests` is empty on Windows.
- R4: Lua/stplug-in registration and verification behavior is unchanged.

## Acceptance criteria

- [x] Unit test: a pre-existing `42_1.manifest` is removed and no new depot
  manifest is written.
- [x] After a sync from the rebuilt app, `steamapps/depotcache` stays empty of
  SteamTron manifests and Steam queues no update.

## Verification

- Removed the eight existing manifests, `appmanifest_228980.acf`, and
  `steamapps/downloading` on this host; restarted Steam and it logged
  `Loaded 0 apps`, with no `Update Queued` and no download activity for 30 s.
- Steam still lists the licensed games via stplug-in.
- Installed 0.1.30, deleted one stplug-in Lua file, created a dummy
  `3468651_…manifest` in `depotcache`, and invoked `sync_steam_library`:
  the dummy manifest was removed, the Lua file was recreated, `depotcache`
  stayed empty, the four ACFs are `StateFlags 2` / `SizeOnDisk 0`, and no
  `downloading` directory or update queue appeared.

## Decisions and risks

- Steam-installed games keep their own depotcache files; only names from the
  current registration are removed.
- Steam may still queue updates for genuine Steam installs; this change only
  stops SteamTron-managed games from triggering them.

## Links

- Related: `0105-windows-stplug-registration`, `0104-bettersteamtools-1-0-9`.
