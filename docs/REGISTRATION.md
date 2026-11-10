# Registration backends and managed add

Status: phase 2 · 2026-11-09
Crates: `steamtron-register`, `steamtron-library::managed`

## Contract

`RegistrationBackend` receives an App ID, display title, Lua manifest and
optional depot manifests. It never downloads content.

| Method | Purpose |
|---|---|
| `id()` | Stable identifier stored in `library.json` (`slssteam-moon`, `bettersteamtools`) |
| `status()` | Installed/ready/steam root/lua dir/details for Doctor |
| `register(request)` | Write the Lua (and any manifests) |
| `verify(app_id)` | Report whether the Lua is present |
| `unregister(app_id)` | Remove only that App ID's Lua |

Adding is rejected while `status().ready` is false; Doctor explains the repair.

## Backends

| Platform | id | Writes |
|---|---|---|
| Linux (SLSsteam Moon) | `slssteam-moon` | `<steam>/config/stplug-in/<appid>.lua`, manifests under `SLSsteam/manifests` and `steamapps/depotcache` |
| Windows (BetterSteamTools) | `bettersteamtools` | `<steam>/config/stplug-in` (or `config/lua`) `<appid>.lua`, manifests under `steamapps/depotcache` |

Registration is idempotent: re-adding rewrites the same files.

## Managed add flow

`add_managed_game` (`steamtron-library`) performs one transaction:

1. Validate the App ID and drive root; reject duplicates.
2. Fetch the Lua archive:
   - `api` — signed-in UMUTron API (Ryuu → Luie → SteamML).
   - `steamml` — public `https://pub-5b6d3b7c03fd4ac1afb5bd3017850e20.r2.dev/<appid>.zip`, no account.
3. Canonicalize `addappid(app, 1, key)` and drop non-Windows depots (best effort;
   a Steam metadata outage keeps the provider Lua unchanged).
4. Register through the backend and verify the file exists.
5. Write the library entry:
   `<drive>/SteamTronLibrary/steamtronapps/{common,compatdata}/<Game>`,
   Steam state `pending_restart`, backend id, launch mode `emulator`.
6. Save `library.json`.

No content is downloaded; the first Play creates the prefix.

## CLI

```bash
steamtron-overlay register status
steamtron-overlay register verify 3468650
steamtron-overlay register add 3468650 --title "Cricket 26" --lua 3468650.lua
steamtron-overlay register remove 3468650
steamtron-overlay managed add 3468650 --title "Cricket 26" \
  --drive /mnt/games --provider steamml
```

Isolated end-to-end run (no real Steam data touched):

```bash
STEAMTRON_STEAM_ROOT=/tmp/st/steam \
XDG_DATA_HOME=/tmp/st/data XDG_CONFIG_HOME=/tmp/st/config \
STEAMTRON_DATA_HOME=/tmp/st/appdata \
steamtron-overlay managed add 3468650 --drive /tmp/st/drive --provider steamml
```

Requirements: `<root>/steamapps` exists and
`/tmp/st/data/SLSsteam/SLSsteam.so` exists.

## Verified

- `steamtron-register`: register/verify/unregister tests for both adapters.
- `steamtron-library`: managed add writes the library entry, rejects duplicates
  and mismatched Lua.
- Real SteamML download + full isolated add for App 3468650: Lua written and
  verified as `slssteam-moon`, library entry `pending_restart`.
