# Storage layout — global app data and per-drive libraries

Status: phase 1 decision · 2026-11-09
Implemented by: `crates/steamtron-core/src/paths.rs` (tests included)

Two roots exist and must never mix:

1. **Global app data** — one per installation, owned by SteamTron.
2. **Per-drive library** — game content, prefixes and fixes, owned by a drive.

UmuTron's data is a **read-only import source**. SteamTron never writes there.

## 1. Global directories

| Purpose | Linux | Windows | Override |
|---|---|---|---|
| Data | `$XDG_DATA_HOME/steamtron` (`~/.local/share/steamtron`) | `%LOCALAPPDATA%\SteamTron` | `STEAMTRON_DATA_HOME` |
| Config | `$XDG_CONFIG_HOME/steamtron` (`~/.config/steamtron`) | `%APPDATA%\SteamTron` | `STEAMTRON_CONFIG_HOME` |
| Cache | `$XDG_CACHE_HOME/steamtron` (`~/.cache/steamtron`) | `%LOCALAPPDATA%\SteamTron\cache` | `STEAMTRON_CACHE_HOME` |

`STEAMTRON_DATA_HOME` remains the compatibility override used by the existing
auth token; an empty value is ignored.

## 2. Data directory tree

```text
<data>/steamtron/
  library.json                 managed library v2 (drives, games, settings)
  config.json                  pre-migration SteamTron config (import source only)
  fixes/<App ID>/              pre-migration fixes (import source only)
  artwork/<sha256>.png|jpg     downloaded artwork (content-addressed)
  tools/
    7zz                        verified 7-Zip helper (GoldBerg extraction)
    goldberg/gse_fork-<tag>/   pinned GoldBerg release
    steamless/steamless-linux/ pinned Steamless tool
  runners/                     downloaded Proton/UMU runners (Linux)
  diagnostics/<game id>/       per-run logs (bounded retention)
  play-history/<game id>.json  recent-play evidence rows
  overlay-upper/<key>/         persistent copy-on-write layers, keyed by game
  launch/                      launch-request.json, launch-session.json, locks
  prefixes/<game id>/          default prefix when a game is not on a managed drive
  backups/                     portable backup archives created by the app
  integrity/                   snapshots used by Installed Files
  api-token.json               account token (kept here for compatibility)
```

Rules:

- **`overlay-upper/` persists** across runs (game writes live there); only the
  transient mount/work directories are removed after Play. They belong under the
  runtime directory (`/tmp/umutron-<uid>`-style), not here.
- **`diagnostics/`** is bounded by retention rules (age + size); clearing it is an
  explicit action.
- **`tools/`** is downloaded once, checksum-verified and shared by every game.
- **`launch/`** holds only one active request/session; stale files are swept on
  start when no launch is active.
- **`artwork/`** is content-addressed; DB rows only ever reference the hash name.
- Everything under the data directory is private to the current user
  (0700 on Unix where we create it).

## 3. Config vs cache

- Config holds user-editable, portable choices only (`config_dir()`), never game
  content. Phase 1 stores settings in `library.json`; the config directory is
  reserved for credentials files that should not be synced with data.
- Cache holds regenerable data (catalog responses, artwork downloads), safe to
  delete at any time.
- The auth token stays at `<data>/steamtron/api-token.json` for backward
  compatibility with the existing sign-in flow.

## 4. Per-drive library

```text
<drive>/SteamTronLibrary/steamtronapps/
  common/<Game>/                 installed content
  compatdata/<Game>/             prefix created by SteamTron
  fixes/<Game>/                  single fixes location
  appmanifest_<App ID>.json      managed manifest (optional)
```

- One fixes location per game; imports copy fixes here and never delete the source.
- `<drive>` is a registered drive root (e.g. `/mnt/krunal`), not the library folder.
- UmuTron's `<drive>/UmuLibrary/umuapps` tree is never modified by SteamTron.

## 5. Coexistence with UmuTron

| UmuTron location | SteamTron reads | SteamTron writes |
|---|---|---|
| `<data>/umutron/library.json` | yes, to plan an import | never |
| `<drive>/UmuLibrary/umuapps/common/<Game>` | yes, as the linked game root | never |
| `<drive>/UmuLibrary/umuapps/fixes/<Game>` | yes, copied into the SteamTron fixes tree | never |
| `<data>/umutron/*` (tools, runners, prefixes, diagnostics) | no | never |

Imported UmuTron games stay **linked** to their existing content; fixes are
copied to `<drive>/SteamTronLibrary/steamtronapps/fixes/<Game>` and the UmuTron
source is kept. This lets both applications run side by side until UmuTron is
retired.

## 6. Environment overrides

| Variable | Effect |
|---|---|
| `STEAMTRON_DATA_HOME` | Replaces the data home (existing compatibility) |
| `STEAMTRON_CONFIG_HOME` | Replaces the config home |
| `STEAMTRON_CACHE_HOME` | Replaces the cache home |
| `STEAMTRON_UMUTRON_LIBRARY` | Import source path override (tests/support) |
| `STEAMTRON_FIXES` | Legacy fixes root override (import source) |

## 7. Safety

- Never write inside an installed game tree; the overlay is the only writable view.
- Every per-game folder is derived from a validated title (`safe_folder`), never
  from an untrusted path.
- Tools are cached per tag/version; an existing folder is only reused after its
  expected layout is present.
- Removing a drive registration never deletes files; removing a game offers
  separate deletion for content, prefixes and fixes.
