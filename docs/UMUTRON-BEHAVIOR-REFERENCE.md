# UmuTron behavior reference for the SteamTron port

Status: living document · Phase 1
Source: deep audit of the UmuTron codebase (2026-11-09)
Scope: behavior to reproduce in Rust/Tauri; UmuTron itself is not modified.

Every bullet cites `file:line` in the UmuTron repository
(`/home/krunaldodiya/WorkSpace/Code/UMUTron-Workspace/UmuTron`). Treat this file
as the contract when implementing SteamTron phases 2-8. Where this document and
an implementation disagree, fix the implementation or amend this file with a
dated decision.

## How to use this document

- **Phase 2 (dual add)** uses §Library model, §Screens (Add flow), §Registration
  references, and the library-adoption migration rules.
- **Phase 3/4 (runtimes)** use §Launch and runtime: command construction, env
  whitelist, prefix modes, request JSON, supervisor lifecycle, evidence.
- **Phase 5 (tooling)** uses §Fixes and Steam-free tooling: overlay lifecycle,
  GoldBerg, Steamless, ZIP limits.
- **Phase 7 (UI)** uses §Library, UI, and interaction: routes, tabs, controller
  model, pickers, confirmations.

## Decisions that override UmuTron

These are product decisions for SteamTron, not UmuTron behavior:

1. **Dual registration on add**: a game is created in the managed library *and*
   registered in the Steam client (SLSsteam-moon / BetterSteamTools). UmuTron has
   no registration backend.
2. **Three launch modes**: `emulator` (GoldBerg), `steam_client` (registered App
   ID + running client), `native`. UmuTron uses prefix mode + `requires_steam`.
3. **Doctor is backend-only**: validates SLSsteam-moon / BetterSteamTools; never
   games, drives, runtimes, fixes. UmuTron has no Doctor.
4. **No Activity**: the global Activity drawer and the per-game Activity tab are
   removed; Debug carries logs and evidence, Doctor carries backend problems.
5. **Tauri/web UI**: GTK widget specifics are replaced; interaction contracts
   (routes, focus, controller, pickers, strings) are preserved.
6. **Rust supervisor** replaces the Python supervisor; process identity, state
   strings, evidence rules, and timing constants are preserved.
7. **Windows runtime is native**: no Wine/Proton; WinFsp provides the overlay.

## Porting status

| Area | Phase 1 status |
|---|---|
| Library document + validation + atomic write | Implemented (`crates/steamtron-core/src/library.rs`) |
| Dry-run importers (legacy config, UmuTron library) | Implemented (`import.rs`) |
| Import commit (safer move, rollback, idempotent) | Implemented |
| Tauri commands (`library_snapshot`, import preview/commit, picker memory) | Implemented |
| U0 shell (tokens, header, no Activity) | Implemented |
| Registration backend, runtime, overlay, GoldBerg, Steamless, Doctor | Phases 2-5 |

Known deltas to align during the port:

- Rust `safe_folder` trims both leading and trailing dots/spaces; UmuTron trims
  trailing only. Existing UmuTron folders are canonical, so follow UmuTron's
  rule before importing real libraries (otherwise folder names change).
- Rust model has no per-game `fixes_path` override: importers convert it to a
  canonical move. Decision: one fixes location, explicit overrides dropped only
  after a safe relocation.
- Managed environment keys (`MANAGED_LAUNCH_ENVIRONMENT`) and the
  `WINEDLLOVERRIDES` merge/cap rules are not yet enforced in Rust; implement in
  phase 3 before any launch.
- `fixes_enabled` defaults to **true** when absent; preserve for imported records.

## Highest-risk invariants (from the audits)

1. Overlay layer order is always `lowerdir=<fixes>:<game_root>` (fixes wins).
2. Overlay temp dirs must not live under `XDG_RUNTIME_DIR` (pressure-vessel remaps it).
3. Supervisor must be a subreaper and gate every kill/evidence decision on
   `/proc/<pid>/stat` start time; PID reuse is the threat model.
4. Two evidence names are distinct: display status (any descendant argv[0]) vs
   selected-game evidence (exact `(dev, ino)` token); history needs the latter
   for 2 continuous seconds.
5. Prefix locks are inode-keyed flocks; transfer the descriptor to the supervisor.
6. Launch child env is a strict whitelist plus managed values; never inherit blindly.
7. GoldBerg/Steamless removal is hash-exact and must never delete user-replaced files.
8. `requires_steam` scrubs `PROTON_DISABLE_LSTEAMCLIENT` and `lsteamclient=` overrides.
9. Steam-prefix mode creates `steamapps/compatdata/<appid>` when missing; only a
   missing Steam install is an error.
10. Fixes DLLs are native-first `<stem>=n,b`, merged with user overrides, capped
    at 1000 characters.

---

## Launch and runtime

All paths relative to `UmuTron/umutron/` unless noted. `launcher.py` below means `UmuTron/umutron/launcher.py`, etc.

### 1. `build_command` contract

**Signature.** `launcher.py:589` — `def build_command(game, root, inherited=None, allow_prepare=False, gamemode=None, overlay=None)`. Returns a tuple `(cmd, launch_cwd, env)` — argv as list of str, cwd as str, env as dict of str→str (`launcher.py:763`).

**Inputs.**
- `game`: dict with `executable`, `working_dir`, `title`, `metadata_app_id`, `installation{mode,confirmed,proton,root,prefix}` and `launch` settings dict.
- `root`: library root (proton-manager, prefixes, diagnostics live under it).
- `inherited`: parent environment dict; default `os.environ` (`launcher.py:653`).
- `allow_prepare`: permits `proton` selectors that are not yet installed ("Play or Launch Installer will download and verify it first") (`launcher.py:637-641`).
- `gamemode`: `None` = read `library.json` → `{"settings": {"gamemode": bool}}` via `is_gamemode_enabled(root)` (`launcher.py:401-409`, `744-745`).
- `overlay`: optional fixes-overlay object exposing `mountpoint` and `root` for path rebasing.

**Defaults resolution.** `defaults(game, root)` (`launcher.py:477-537`): calls `validate_settings` (`launcher.py:318-374`, allowed keys = `FIELDS` set at `launcher.py:19-40`, unknown key → `"Invalid direct launch settings."`), merges `proton_environment`+`environment` via `effective_environment` (`launcher.py:288-316`), resolves the proton selector (`launcher.py:480-497`):
- `global_selector(root)` reads `library.json` → `settings.default_proton`, default `"UMU-Latest"` (`runner_selection.py:62-73`).
- `effective_selector(game, global_default)` (`runner_selection.py:45-53`): pinned value `"default"` → global fallback; else game `launch.proton` → `installation.proton` → fallback.
- `_selectable_proton` (`launcher.py:463-474`): `True` for selectors `UMU-Latest`/`GE-Latest`, any valid `release:Family:Tag` (`parse_release`, `runner_selection.py:18-29`, families `GE-Proton|UMU-Proton|Proton-CachyOS|Valve-Proton`), or an absolute directory containing a `proton` file.
- Stale managed runner under `<root>/proton-manager/runners`: attempt `ProtonManager(root/"proton-manager").cached_selector_for_target(dir_name)` (local-only, `proton_manager.py:503-528`); else `installation.proton` or global default; final fallback literal `"UMU-Latest"` (`launcher.py:487-497`).
- Prefix-mode resolution \(see section 3\); `runner` = `launch.runner` or `shutil.which("umu-run")` (`launcher.py:508-516`).

**Executable checks.** Missing/non-absolute/non-file executable → `"Choose an existing game executable."` (`launcher.py:592-593`). ELF check first 4 bytes `b"\x7fELF"` → `is_native_executable` (`launcher.py:578-585`); native launchers require exec permission, repaired via `payload_pipeline.ensure_executable`, else `"The game launcher is not executable and could not be repaired."` (`launcher.py:594-597`). Non-native requires runner absolute + file + `X_OK`, else `"Install umu-run or choose its executable path in Direct Play."` (`launcher.py:598-599`).

**cwd selection.** `cwd_source = game["working_dir"] if set else exe.parent`; must be absolute and existing dir, else `"The working directory does not exist."` (`launcher.py:600-602`).

**Overlay rebasing.** If `overlay` is given, both `launch_exe` and `launch_cwd` are re-rooted under `overlay.mountpoint` by replacing the `overlay.root` prefix; a `ValueError` from `relative_to` (path outside overlay) falls back to the original host path (`launcher.py:604-612`). The caller (`Launcher.start`) additionally wraps argv via `overlay.wrap(argv, cwd)` and maps cwd back to the host path (`launcher.py:955-959`).

**Native vs Proton.**
- Native: `proton = "Native"` (`launcher.py:615`), no `WINEPREFIX/PROTONPATH/PROTON_LOG*/Steam*` env is set at all (`launcher.py:681-688` guard `if not native:`), prefix must be `None` (`launcher.py:648`), argv `[launch_exe, *arguments]` (`launcher.py:746`).
- Proton: argv `[runner, launch_exe, *arguments]` (`launcher.py:746`). Proton selection: `parse_release(proton)` → resolve to installed path via `ProtonManager(...).installed_release(proton)` (`proton_manager.py:478-500`, scans `tools/` for a directory with a ≤16 KiB non-symlink `umutron-release.json` receipt matching the selector and containing `proton`); unresolved + `not allow_prepare` → `"This Proton version is not installed yet. Play or Launch Installer will download and verify it first."` (`launcher.py:621-642`). A literal path must be an absolute dir containing `proton`, else `"Choose a Proton folder containing the proton launcher, or UMU-Latest / GE-Latest."` (`launcher.py:643-647`).

**Prefix validity (umu mode).** Steam mode on a native exe → `"Steam prefix mode is only available for Windows games."` (`launcher.py:649-650`). Non-native: prefix must be absolute and outside the game folder (`"Use a dedicated prefix outside the game folder."`); an existing prefix must be a directory, not symlink (`"The prefix must be a dedicated directory, not a file or symbolic link."`); non-empty without the marker file `MARKER = ".metadata-manager-prefix"` (`launcher.py:43`) → `"This folder already contains data. Choose a new empty prefix; existing game prefixes are preserved."`; any symlinked parent → `"Choose a prefix without symbolic-link parents."` (\l`launcher.py:651-66+`).

**Gamemode/Mangohud/Gamescope prefixes** (order applied: gamemode → mangohud → gamescope outermost):
- `gamemoderun` prepended when enabled and `shutil.which("gamemoderun")` exists; `gamemode_binary()` = `shutil.which("gamemoderun")` (`launcher.py:397-398`, `747-748`).
- `mangohud` prepended when the setting was enabled and `which("mangohud")` (\l`launcher.py:749-750`).
- `gamescope` prepended with args: `-w <w> -h <h>` when both `gamescope_w` and `gamescope_h` truthy; `-W <out_w> -H <out_h>` when both output sizes truthy; `-F fsr` + `--sharpness <n>` (sharpness only when not `None`) when `gamescope_fsr`; then `--`, then the existing cmd (\l`launcher.py:751-762`).

### 2. Environment whitelist

- **Whitelist copied from parent** exactly (`launcher.py:651-679`): `HOME, USER, LOGNAME, PATH, LANG, DISPLAY, WAYLAND_DISPLAY, DBUS_SESSION_BUS_ADDRESS, XAUTHORITY, XDG_RUNTIME_DIR, XDG_SESSION_TYPE, XDG_DATA_HOME, XDG_CONFIG_HOME, XDG_CACHE_HOME, XDG_DATA_DIRS, PULSE_SERVER, PIPEWIRE_REMOTE, DRI_PRIME, __NV_PRIME_RENDER_OFFLOAD, __GLX_VENDOR_LIBRARY_NAME, PRESSURE_VESSEL_FILESYSTEMS_RW`, plus **any key with prefix `LC_`**.
- **Set unconditionally:** `PYTHONUNBUFFERED=1` (\l`launcher.py:680`).
- **Proton-only block** (`if not native:`, \l`launcher.py:681-688`): `WINEPREFIX=<prefix>`, `PROTONPATH=<resolved proton>`, `UMU_LOG="debug"`; `PROTON_SET_GAME_DRIVE="0"` only for prefix-mode `umu` whose proton folder name starts with `proton-cachyos-` (\l`launcher.py:683-686`); `PROTON_LOG="-all,err+all,warn+seh"` and `PROTON_LOG_DIR=str(<root>/diagnostics/<game_id>)` (\l`launcher.py:687`); then all validated user `settings["environment"]` values \l`launcher.py:688`). **Rules:** user env values are validated by `validate_environment` (\l`launcher.py:225-246`): ≤32 entries, names match `[A-Za-z_][A-Za-z0-9_]*`, each value ≤1024 chars and no NUL/CR/LF, total key+value ≤8192, and no key may shadow the **managed** set `MANAGED_LAUNCH_ENVIRONMENT` (`GAMEID, HOME, PATH, PRESSURE_VESSEL_FILESYSTEMS_RW, PROTONPATH, PROTON_LOG, PROTON_LOG_DIR, PROTON_SET_GAME_DRIVE, STEAM_COMPAT_APP_ID, STEAM_COMPAT_CLIENT_INSTALL_PATH, STEAM_COMPAT_DATA_PATH, STEAM_COMPAT_INSTALL_PATH, STEAM_COMPAT_LIBRARY_PATHS, STORE, SteamAppId, SteamGameId, SteamOverlayGameId, SteamPath, UMU_LOG, WINEPREFIX`) (\l`launcher.py:48-69`).
- `WINEDLLOVERRIDES` composition in `effective_environment` (\l`launcher.py:288-316`): `WINEDLLOVERRIDES` > `dll_overrides` > `detected_dll_overrides(exe)` (`dbghelp=n,b`, `winmm=n,b`, and for false-positive crack-detection: `lsteamclient=d,steamclient64=n,b,steamclient=n,b`, \l`launcher.py:443-460`), then merged with fixes-overlay DLL stems as `stem=n,b` sorted, newest append, ≤1000-char cap (\l`launcher.py:249-286`). `requires_steam` scrubs `PROTON_DISABLE_LSTEAMCLIENT` and any `lsteamclient=` override entry (`launcher.py:299-309`). `steam_appid.txt` is **pre-seeded** in `cwd_source/<app_id>\n` when absent and no overlay (`launcher.py:738-742`).
- **Steam-mode key set** (`launcher.py:690-737`): `GAMEID=f"umu-{app_id}"`, `SteamAppId=SteamGameId=STEAM_COMPAT_APP_ID=<app_id>`, `STEAM_COMPAT_DATA_PATH=<compatdata/app_id>`, `STEAM_COMPAT_CLIENT_INSTALL_PATH=<Steam root>`, `SteamPath=<Steam root>`, `SteamOverlayGameId=<app_id>`, `STORE="steam"`; when the game actually lives in a real `<library>/steamapps/common/<game>` layout, plus `STEAM_COMPAT_INSTALL_PATH=<install>` and `STEAM_COMPAT_LIBRARY_PATHS=<steamapps dir>`; `PRESSURE_VESSEL_FILESYSTEMS_RW` is accumulated with library, install, and overlay mounts joined with `:` (`launcher.py:721-732`).
- **Umu-mode Steam keys:** `STEAM_COMPAT_DATA_PATH=<prefix>`; first existing of `~/.local/share/Steam`, `~/.steam/steam`, `~/.steam/root` becomes `STEAM_COMPAT_CLIENT_INSTALL_PATH` (`launcher.py:733-737`).

### 3. Prefix modes

- `PREFIX_MODES = {"umu", "steam"}` (`launcher.py:45`). `selected_prefix_mode(game, settings=None)` (`launcher.py:190-193`): explicit setting wins if in `PREFIX_MODES`; else `"steam"` when `_steam_app_id(game)` yields a numeric App ID, else `"umu"`.
- **App-id extraction** `_steam_app_id(game, executable=None)` (`launcher.py:184-187`): `game["metadata_app_id"]` or `detect_steam_appid(exe)`, strip `umu-`, accept `[1-9]\d{0,9}`; else `None`. `detect_steam_appid` (`launcher.py:412-440`): stored (`steam_appid.txt` numeric, next to the exe) >> > locate `steamapps` ancestor and match the `installdir` case-insensitively against the exe's parent folder name to find the right `appmanifest_*.acf` `appid`.
- **`steam_prefix_paths(app_id, home=None)`** (`launcher.py:128-159`): strips `umu-`; non-numeric → `"Steam prefix mode requires a numeric Steam App ID."`; probes `~/.local/share/Steam`, `~/.steam/steam`, `~/.steam/root`; a root containing `steamapps/` resolves via `_steam_library_for_app` (client library then `libraryfolders.vdf` `"path"` entries; manifest-first, fallback to `compatdata/<app_id>` which may be obsolete; `launcher.py:71-114`). A Steam client root **without** a `steamapps/` dir is still usable: it falls back to `<root>/steamapps` and `common`, creating it later (`launcher.py:132,150-155`). Missing Steam entirely → `"Steam prefix mode requires a local Steam installation."`.
- **Layout produced** by `_steam_prefix_layout(root, steamapps, install, app_id)` (`launcher.py:116-125`): keys `root`, `steamapps`, `library` (`steamapps.parent`), `install`, `data = steamapps/compatdata/<app_id>`, `prefix = data/pfx`.
- **Created vs reused:** in both modes the launcher only records paths; the supervisor creates vs reuses: umu mode `mkdir(parents=True, mode=0o700)` and writes the `MARKER` file `\n` (`launch_supervisor.py:283-289`). Steam mode: `new_steam_prefix = not (prefix/system.reg).is_file()`; for `operation=="play"` and a new prefix, a wineboot `proton run wineboot -u` runs with clean `STEAM_COMPAT_DATA_PATH/STEAM_COMPAT_CLIENT_INSTALL_PATH` env (`steam_prefix_setup`, `launcher.py:162-181`, removing `LD_AUDIT`/`LD_PRELOAD`), requiring `pfx/system.reg` after success (`launch_supervisor.py:77-100,297-304`).

### 4. Launch request JSON

`Launcher.start()` writes `<root>/launch-request.json` via `atomic_write` (`launcher.py:987-991`). Fields consumed by `launch_supervisor.main()` (`launch_supervisor.py:139-489`):

| field | type | notes |
|---|---|---|
| `operation` | str | one of `play`, `installer`, `runtime` (`launch_supervisor.py:225,249,178`; validated in `Launcher.start` `launcher.py:809-810`) |
| `session_id` | str (UUID) | matches `cancel-request.json` and history rows; also recorded in the state snapshot |
| `game_id` | str (UUID) | required |
| `title` | str | required |
| `executable` | str | host or overlay-visible exe path (rebased under overlay if possible, `launcher.py:918-925`) |
| `argv` | list[str] | full command to spawn (launcher- or overlay-wrapped/adjusted) |
| `cwd` | str | host cwd (host-mapped when overlaid) |
| `env` | dict[str,str] | full prepared environment (key source of `WINEPREFIX/PROTONPATH/PROTON_LOG_DIR/prefix_mode/Steam keys`) |
| `prefix_mode` | `"umu"`/`"steam"` | selected by `selected_prefix_mode(game)` at request-build time |
| `steam_prefix` | `null` or `{app_id: str, root: str, data: str}` | only for `prefix_mode=="steam"` |
| `runner` | str | `launch.runner` or `shutil.which("umu-run")` |
| `game_root` | str | installation root (Steam `_CommonRedist` scans under it) |
| `record` | str | absolute path of `launch-session.json` |
| `requires_steam` | bool | triggers `require_steam_client()` pre-check |
| `overlay` | (optional) dict `{backend: str, directory: str, mountpoint: str}` | only when a fixes overlay was mounted (`launcher.py:932-934`) |
| `prelaunch` | optional dict `{path: str, sha256: str}` | from `prelaunch.hook_for(game)` |

### 5. Supervisor lifecycle

**Process.** Spawned by `Launcher.start` as `python launch_supervisor.py <launch-request.json>` with `start_new_session=True`, stdin/stdout/stderr devnulled, passed file-descriptors of `launch.lock` and the prefix lock (`launcher.py:1028-1041`). `main()` immediately becomes a subreaper: `prctl(PR_SET_CHILD_SUBREAPER=36, 1, 0, 0, 0)`; on failure `"Unable to enable safe launch supervision."` (`launch_supervisor.py:142-143`). All orphaned descendants are adopted by the supervisor, not the desktop app.

**Run record.** `record` path from the request; the record is a dict written atomically via `save()` (`launch_supervisor.py:175-181`); for `operation=="installer"` every save is also mirrored to `installation-sessions/<game_id>.json`. Record fields: `operation, session_id, game_id, title, state, logs[], code` plus optional `proton, supervisor_pid, supervisor_start, preparation_stage, downloaded_bytes, total_bytes, diagnostics_dir, selected_game_observed, runtime_verified`. Logs is a 200-line deque (`launch_supervisor.py:161`). The desktop reader (`Launcher.current`, \l`launcher.py:849-880`) rejects records >500 KB and only accepts `state ∈ ACTIVE ∪ {"Finished","Error","Stopped"}` where `ACTIVE = {"Preparing", "Downloading runtime", "Running", "Stopping"}` (`launcher.py:42`); recency state `"Not started"` if no matching record (`launcher.py:883-886`).

**State names & transitions.**
- `"Preparing"` initial (`launch_supervisor.py:148`); a pending `cancel-request.json` short-circuits to `"Stopped"` + `"Operation cancelled before execution."` before touching runners or prefixes (`launch_supervisor.py:224-227,320-323`).
- `"Downloading runtime"`: for `operation=="runtime"` on a Microsoft runtime prepare (which runs an interactive installer the user must accept, `launch_supervisor.py:249-271`); also produced when observed UMU output lines contain `download`, `restoring runtime`, `setting up unified`, `updating steamrt` before it becomes `"Running"` (`launch_supervisor.py:395-402`); and by `prepare_runner` progress callbacks (`launch_supervisor.py:238-244`; `runner_preparation.py:10-39` resolves a `release:` selector and downloads/verifies it into `proton-manager`, returning a runner lease fd held until the supervisor exits).
- `"Running"`: set when log line contains the literal substring `waitforexitandrun` (`launch_supervisor.py:404`) or when the bounded running-evidence check succeeds (`launch_supervisor.py:421-428`).
- `"Stopping"`: on user stop (SIGTERM) or lost overlay/cancel, a stop loop SIGKILLs after 3 s and force-rechecks ownership before killing each PID (`launch_supervisor.py:415-427`).
- Termination states: `"Stopped" if stopping else ("Finished" if code == 0 else "Error")` + message `"Launch stopped."` or `"UMU exited with code <code>."` (`launch_supervisor.py:455-456`); for `operation=="runtime"`, additionally `runtime_verified` from `umutron.runtime_maintenance.verify` (accepted exit codes `0` or `3010`, `launch_supervisor.py:457-470`).

**Cancellation/stop semantics.** Two channels from the desktop app: pre-start cancellation writes `cancel-request.json` `{"session_id": …, "game_id": …}` when the record shows `"Preparing"` and no supervisor pid (`launcher.stop`, `launcher.py:1053-1063`); the supervisor rechecks this file (≤4 KB, must match session+game, `launch_supervisor.py:211-218`). A live supervisor receives `SIGTERM` directly (`launcher.py:1065`), which only sets a flag (`launch_supervisor.py:163`); it then escalates children to `SIGKILL` after 3 s. The desktop `stop` errors: `"This game has no owned active launch."` (`launcher.py:1053-1055`), `"The launch supervisor no longer exists. Review the operation status before retrying."` when pid/identity lookups fail (`launcher.py:10610`). The runtime/session errors: `"A game is already active. Stop or finish it before launching another."` (`launcher.py:902`).

**Evidence polling.**
- `request_target_executable(request)` (`launcher.py:825-840`): `request["executable"]`, else `argv[2]` if `argv[0]` basename is `gamemoderun`, else `argv[1]`; both supervisor and UI probe agree via this single function.
- `running_game_evidence(supervisor_pid, supervisor_start, executable)` (`launcher.py:784-822`): requires int pid, nonempty start string, and `/proc/<pid>/stat` start-time match; walks the supervisor's descendants (≤100 levels, ≤8192 /proc entries, skipping zombies); a descendant's non-empty argv[0] basename (casefolded; `\\`-normalized for Wine paths) must equal the target's basename, stopped only once the pid's identity matches — it is purposely cmdline-argument-blind so wrapper argv cannot spoof it.
- `selected_path_evidence` for history: `process_evidence.selected_game_evidence(supervisor_pid, supervisor_start, executable, prefix)` (`process_evidence.py:46-113`) returns a stable token `(pid, start_time, st_dev, st_ino)` only when exactly one descendant runs the selected executable identified as **argv[0]**: resolved via `/proc/<pid>/cmdline`, matched either (a) a real Unix path or a Wine `X:/...` drive path mapped through `<prefix>/dosdevices/x:` and compared by stat identity `(st_dev, st_ino)` against a regular-file `executable`; or (b) a renamed Wine game process recognized via `/proc/<pid>/comm` where `title` basename matches the `comm` (`process_evidence.py:34-44,46-113`). Ancestor chains are rechecked against start times to reject PID reuse.
- UI evidence polling: `Launcher.current()` polls the identical evidence and can flip a `"Preparing"`/`"Downloading runtime"` record to `"Running"` for display, cached for 0.5 s keyed by `(supervisor_pid, supervisor_start, game_id)` (`launcher.py:860-878`).
- Supervisor in-loop polling cadence: save every 0.2 s, evidence every 0.5 s, overlay mountalive check every 0.5 s (`launch_supervisor.py:368-378,417-440,444-447`); a vanished overlay mount stops the launch with a log line (`launch_supervisor.py:377-379`).

**Recent-play recording conditions.** `PlayMilestone.observe(operation, evidence, stopping, now)` (`play_history.py:59-77`) returns `True` only for `operation=="play"` with a fresh, non-stopping evidence token held for **2 continuous seconds** with the same token; a different token restarts the timer; once recorded, no repetition. When it fires the supervisor writes the play-history row via `play_history.record(root, game_id, session_id, observed_at)` (`play_history.py:9-19` → `play-history/<game_id>.json` with `{"game_id", "session_id", "observed_at"}`, both validated UUIDs; timestamp must be a positive int/float), and sets `selected_game_observed: true`; on write error it only logs `"Recent-play history could not be saved; the game continues."` (`launch_supervisor.py:430-439`).

**Diagnostics handling.** `PROTON_LOG_DIR` folder is created `0o700` and exposed as `diagnostics_dir` (`launch_supervisor.py:291-296`); supervisor startup calls `diagnostics.cleanup(root)` to age out old logs and `diagnostics.reset_game(root, game_id)` for a fresh per-game log (`launcher.py:908-912`); on exit `cleanup` removes the request file etc. `cleanup_request(request)` (fixes overlay teardown) runs in the finally-block before the final save (`launch_supervisor.py:475-489`). Output lines go through `clean_log` (`launcher.py:766-772`): ANSI-CSI stripped; `DEBUG`-tagged `KEY=` output suppressed; then tokens, passwords, secrets, authorization values redacted and URLs querystrings rewritten; lines truncated to 2000 chars. In-loop reads are line-buffered 64 KiB with a pending buffer carrying >8192-byte partial lines (`launch_supervisor.py:385-394`).

### 6. Failure modes — exact error strings

All exact strings from the sources cited above, plus these additional literal failures:
- `"A game is already active. Stop or finish it before launching another."` (`launcher.py:901-902`, flock `LOCK_EX|LOCK_NB` on `launch.lock` held across supervisor lifetime).
- `"Prefix changed before launch. Existing data was preserved."` (supervisor umu-mode pre-check when the prefix became non-empty without marker or symlink, `launch_supervisor.py:283-289`).
- `"Steam prefix selection changed before launch. Existing data was preserved."` (`launch_supervisor.py:274-280`, `valid_steam_prefix_request`, `launch_supervisor.py:38-50`).
- `"Proton prefix setup failed with code <n>."` / `"Proton prefix setup completed without creating a Wine prefix."` (`launch_supervisor.py:97-100`).
- `"Operation cancelled before execution."` (`launch_supervisor.py:226,322`).
- `"Pre-launch script cancelled."` / `"Pre-launch script did not finish within <timeout> seconds."` / `"Pre-launch script failed with exit code <n>. The game was not started."` (`prelaunch.py:106-152`; the hook runs on the host as the current user with env including `UMU_GAME_DIR/UMU_GAME_ID/UMU_TITLE/UMU_PREFIX/UMU_DRIVE_C/UMU_PROTON`, 120 s timeout, ≤1 MiB script, ≤100 emitted lines, must be user-owned, non-world-writable, non-symlink regular executable file matching its approved SHA-256; on refusal `"The pre-launch script changed since it was approved. Review it and approve it again."`).
- Prefix guard (held as an inode-keyed flock under `$XDG_RUNTIME_DIR` or `/tmp/umutron-<uid>`/`umutron-prefix-locks`, `prefix_guard.py:65-91`): `"Another operation owns this prefix, possibly in another library."`, `" Processes still use this prefix (PIDs <n>, …). Close them before maintenance or launch."`, `"Cannot verify that this prefix is idle. Same-user process inspection was denied; maintenance/launch is blocked."` (`prefix_guard.py:51-63,65-88`); process conflicts scan `/proc` for other same-user pids with `WINEPREFIX` or `STEAM_COMPAT_DATA_PATH` resolving to the prefix (or its `pfx`) or cwd inside it (`prefix_guard.py:10-49`).
- `"Unable to enable safe launch supervision."` (`launch_supervisor.py:143`).
- `"Selected Proton runner is unavailable. Choose another version in Setup."` (`runner_preparation.py:35-38`); `"Proton preparation cancelled."` (`runner_preparation.py:13-14,31`).
- `"Steam redistributables require the configured umu-run executable."` (`launch_supervisor.py:114-115`). Installer exit ≠ 0 is logged as continuing: `"<file> exited with code <n>; continuing."` (`launch_supervisor.py:136`). CommonRedist list is only the known `_CommonRedist/vcredist & DirectX` families. Marker: `data/.umutron-redists-done` (`launch_supervisor.py:106-137`).

### 7. Porting notes (Rust supervisor/runtime adapter)

- **Must preserve process identity checks.** Every kill signal and every evidence decision is gated by `/proc/<pid>/stat` start-time (field 22, `launcher.py:775-781`) matching the recorded `supervisor_start`. A Rust supervisor must record `(pid, starttime)` at spawn and refuse to act when either becomes stale — PID reuse is the central threat model.
- **Subreaper semantics.** `PR_SET_CHILD_SUBREAPER=36` (`launch_supervisor.py:142`) is what makes orphan adoption (Wine/proton helper processes) safe for `waitpid(-1, WNOHANG)` reaping (`launch_supervisor.py:409-415`). In Rust, implement via `libc::prctl` before any spawn, and never let the supervisor exit while descendants run; on terminate states, SIGTERM children and SIGKILL after a 3-second grace (`launch_supervisor.py:420-427`).
- **Two-name evidence must not be collapsed.** `running_game_evidence` (any descendant with matching argv[0] basename) is display-only status; `selected_game_evidence` (argv[0] path or Wine dosdevices mmap, `(dev, ino)` matched to a **regular file** at `executable`, exactly one token) gates play-history rows. A Rust port missing either branch risks recording plays for launchers/wrappers or rejecting renamed Wine game processes (`process_evidence.py:34-44,46-113`).
- **Timing constants worth preserving:** save every 0.2 s, overlay and evidence probes every 0.5 s, selection-check caching every 0.5 s from the UI path, `PlayMilestone` requires 2 s of uninterrupted evidence, prelaunch timeout 120 s, log-lettering cap 200 entries, output buffer 64 KiB, `/proc` walk cap 8192, ≤100 ancestor steps.
- **Prefix and launch locking.** `launch.lock` is a `flock(LOCK_EX|LOCK_NB)` file descriptor passed to the supervisor so the desktop app cannot start a second launch while the supervisor lives (`launcher.py:847-870,1041`); the independent `umutron-prefix-locks` dir under `$XDG_RUNTIME_DIR` (or `/tmp/umutron-<uid>`) holds per-prefix (sha256 of resolved path) `.lock` files held by the supervisor until it exits (`prefix_guard.py:65-91`). Preserve lock-ownership transfer via `pass_fds`/dup in Rust.
- **Run record is the UI contract.** The record is JSON with fixed state strings, plus the `selected_game_observed` and `diagnostics_dir` extras. Keep state-string values (`"Preparing" | "Downloading runtime" | "Running" | "Stopping" | "Finished" | "Error" | "Stopped"` and UI fallback `"Not started"`) exactly, since the desktop reader validates them (`launcher.py:42,849-880`).
- **Environment isolation traps (common porting mistakes).** The child env is **not** inherited wholesale; strict whitelist + `LC_*` prefix (`launcher.py:651-679`) plus managed values. Overlay rebases exe/cwd, and missing mount detection must stop the launch (`launch_supervisor.py:372-379`). Proton log redirection env (`PROTON_LOG`, `PROTON_LOG_DIR`) is unconditionally set for all Windows launches, not a user choice (`launcher.py:687`). `steam_appid.txt` is written inside the game cwd when missing (`launcher.py:738-742`) — re-specify this behavior precisely.
- **Locking traps.** `flock` is per-fd (not per-process) in this design: acquire the same lock twice from different fds of the same process fails; a Rust implementation on a different OS (or Windows) must replicate the cross-process mutex via native file ranges or advisory locks because `flock(LOCK_NB)` from a stale parent can always be refused by a still-running child via the same file (never via a PID file).
- **Unknown/unmatched /proc-dir volume caps** — treat as `unknown`: exact `/proc` walk cost on huge host systems is the same cap (8192 entries) regardless of host; distinguish `unknown` for behavior beyond 8192 running processes (`launcher.py:797`, `process_evidence.py:60`).

All cited line numbers from the current package sources read in this session; nothing inferred or projected.

---

## Fixes and Steam-free tooling

### 1. Fixes location rules

### Canonical path derivation
- Managed layout is `<drive>/UmuLibrary/umuapps/` with `common/<Game>`, `compatdata/<Game>`, `<drive>/UmuLibrary/umuapps/appmanifest_<key>.json`; layout name constant `_LAYOUT = "UmuLibrary/umuapps"`, manifest prefix `"appmanifest_"`, max manifest size 256 KiB (game_assets.py:29-31).
- Folder-name canonicalization: `_safe_folder(title)` replaces `[\\/:*?"<>|\x00-\x1f]+` with a space, collapses whitespace, strips trailing dots, rejects empty/`.`/`..`, and truncates to 120 chars (game_assets.py:45-53).
- The single fixes location is `umuapps/fixes/<Game>`; `fixes_path()` returns None when absent or empty, raises elsewhere (fixes_overlay.py:209-231). App manifests record it as `fixes_directory: "fixes/<folder>"` (fixes_overlay.py:233-235; game_assets.py:216, 261).
- `_library_fixes_root` finds `umuapps` by walking the root's parents (requires sibling `common/` dir) and falls back to walking for `<ancestor>/UmuLibrary/umuapps` (fixes_overlay.py:61-75). `canonical_fixes_dir` = that `fixes` root joined with the game root's own folder name (fixes_overlay.py:77-82).

### Game-root validation (`_game_root`, fixes_overlay.py:162-207)
- Root comes from `game["installation"]["root"]`; must be absolute, no `..` (fixes_overlay.py:173-176).
- Symlink safety: `raw`, `raw.parent`, `raw.parent.parent` (common/ and umuapps/) must not be symlinks (fixes_overlay.py:177-178).
- Enforced layout: `root.parent.name == "common"` and `root.parent.parent.name == "umuapps"`, and the folder name must equal `_safe_folder(name)` exactly, else error (fixes_overlay.py:193-207).
- Linked games: a game elsewhere but still under an UmuTron drive (found via `_library_fixes_root`) is accepted with fixes kept in that drive's `umuapps/fixes/<Game>`; a game neither canonical nor linked requires an explicitly configured `fixes_path` else `FixesError` (fixes_overlay.py:191-198).

### Configured `fixes_path` handling
- `_configured_fixes` reads `game["fixes_path"]`: expanded, must be absolute, no `..`, not `/`, not itself or its parent a symlink; not required to exist (fixes_overlay.py:30-45). When set it overrides the canonical directory in `fixes_path` (fixes_overlay.py:214-216), import (fixes_overlay.py:282-284), Goldberg `_fixes_directory` (goldberg.py:274-281) and Steamless `_fixes` (steamless.py:255-262).

### `fixes_enabled`
- Per-game bool `game["fixes_enabled"]`; **absent defaults to True** (compat with pre-switch records); non-bool raises `FixesError` (fixes_overlay.py:47-57). `OverlaySession.for_game` returns None (launch unchanged) when disabled, when no `installation.root`, when no fixes, or for a linked game without a chosen fixes folder (fixes_overlay.py:692-712 approx — `for_game` in `OverlaySession`).

### Legacy relocation & identity collapse
- Old builds stored fixes under `<app data>/fixes/<App ID>`; `adopt_legacy_location` relocates only when the configured fixes path's parent equals that legacy root, targeting the canonical dir (fixes_overlay.py:143-159).
- `relocate_folder(source, target)` never merges (fixes_overlay.py:116-140):
  - target exists non-dir → None; target non-empty → only if `_trees_identical` (same file-name set, same sizes, byte-identical in 1 MiB chunks, symlinks excluded — fixes_overlay.py:84-113), in which case the source tree is deleted (identity collapse) and target returned; differing trees are left untouched (None). Target exists **and is empty** → `rmdir`, then move.
  - move into `target.parent` created `0o755`; move failure raises `FixesError`.
- Library-adjacent scan also relocates any loose fixes folders to canonical targets (library.py:282-326 calls `adopt_legacy_location` and `relocate_folder`).

### Orphan fixes
- `orphan_fixes(roots)` lists `umuapps/fixes/<name>` folders with no matching `umuapps/common/<name>` dir, with `{"name","path","files","bytes"}` (fixes_overlay.py:484-531). `remove_orphan_fixes` refuses symlinks, paths outside `umuapps/fixes`, and folders whose game still exists (fixes_overlay.py:534-551).

### Safety checks summary
- Every consumer re-checks symlinks on the fixes dir and its parent before use (fixes_path fixes_overlay.py:217-218; import fixes_overlay.py:285-296; Goldberg goldberg.py:517-523; Steamless steamless.py:337-343). Overlay dirs are created `0o700` and verified to be owned by the current UID on Linux (see §2). `remove_fixes`/`remove_orphan_fixes` never follow symlinks (fixes_overlay.py:471-480, 534-551).

### 2. Overlay lifecycle

### Backend
- Only backend: `FuseOverlayBackend` (`name="fuse-overlayfs"`), because an outer bubblewrap namespace breaks UMU/pressure-vessel's own sandbox (fixes_overlay.py:592-599). Procurement check: `fuse-overlayfs` and `fusermount3`/`fusermount` on PATH, with install-hint message (fixes_overlay.py:601-612). `select_backend` honors env `UMUTRON_OVERLAY_BACKEND` (fixes_overlay.py:649-662); `capabilities()` returns the summary for UI (fixes_overlay.py:664-672).

### Runtime & persistent locations
- Runtime parent `_runtime_parent`: env `UMUTRON_OVERLAY_DIR` (must be absolute) else `TMPDIR`-based; if TMPDIR is not absolute or is under `XDG_RUNTIME_DIR`, falls back to `/tmp` (then `/var/tmp`); final base `<tmp>/umutron-<uid>/.overlay`. Deliberately **not** `XDG_RUNTIME_DIR` because pressure-vessel remaps it inside the container (fixes_overlay.py:554-573).
- Persistent upper parent `_persistent_parent`: `UMUTRON_OVERLAY_DATA_DIR` (absolute required) else `$XDG_DATA_HOME/umutron/overlay-upper` (fixes_overlay.py:574-583).
- Private key per game: `sha256(str(game.id or metadata_app_id or game_root))` hex, first 32 chars (fixes_overlay.py:566-570).

### Session (OverlaySession, fixes_overlay.py:675-780 approx)
- Paths: `directory = base/<gameFolderName>-<hex16>`, `mountpoint = directory/merged`, `upper = persistent_parent/<key>`, `work = upper.parent/.<key>-work-<hex8>` (fixes_overlay.py:681-688).
- `create()`: mkdirs at `0o700`, sets upper `exist_ok=True` (persists across sessions), then ownership check — every created dir must not be a symlink and have `st_uid == os.getuid()` — and chmod `0o700`; symlink post-check on directory/mountpoint/upper/work (fixes_overlay.py:715-740).
- `mount()`: `fuse-overlayfs -o lowerdir=<fixes>:<game_root>,upperdir=<upper>,workdir=<work> -o nosuid,nodev <mountpoint>`, 20 s timeout; failure or timeout cleans up and raises (fixes_overlay.py:614-648). Layer order: **fixes is the higher layer, game root the lower one**.
- `verify()`: requires `os.path.ismount(mountpoint)`, then a write probe `.umutron-write-probe` inside `merged/`; the probe file must appear in the **upper** layer — proving copy-on-write into the private persistent layer — then it's unlinked (fixes_overlay.py:758-775).
- Upper layer persistence: upper is keyed by game, never deleted by session lifecycle; only the per-session `work` dir and runtime `directory` are transient.

### Cleanup / unmount
- `cleanup()`: unmount mountpoint via `fusermount3 -u`, `fusermount -u`, then `fusermount3 -uz` (lazy) with 10 s timeout each, until not a mount (fixes_overlay.py:777-806); removes `directory` and `work` (guarded: work name must start `.<upper.name>-work-`, fixes_overlay.py:779-784).
- `cleanup_request(request)` removes the launch request's `overlay.directory` (fixes_overlay.py:818-823). `sweep()` removes **all** stale directories under the runtime base, calling caller-responsibility note about the session lock (fixes_overlay.py:826-842). launcher calls `sweep()` when no launch is active (launcher.py:851-854).
- The launch supervisor additionally polls the mount every 0.5 s and stops the launch if the overlay mount disappears ("The read-only fixes overlay was lost; stopping this launch.") (launch_supervisor.py:337-350).

### Path rebasing — what the launcher passes to the runtime
- `overlay.wrap(argv, cwd)` returns argv unchanged (FUSE mount is directly usable); `overlay.host_path(cwd)` returns the cwd unchanged (fixes_overlay.py:745-755).
- `build_command(..., overlay=...)` rewrites the launch executable and cwd onto `overlay.mountpoint / <relative to game root>` when inside the root (launcher.py:607-616), and adds `overlay.mountpoint` to `PRESSURE_VESSEL_FILESYSTEMS_RW` (launcher.py:727-731).
- Launch request gains:

```json
"overlay": {"backend": "fuse-overlayfs", "directory": "<runtime dir>", "mountpoint": "<runtime dir>/merged"}
```
and `"executable"` is rebased onto the mountpoint (launcher.py:993-1023; overlay dict at launcher.py:1005-1009).
- Overlay creation/verify happens before `build_command`; any failure cleans up (launcher.py:934-955). `fixes_dll_overrides(game)` derives `WINEDLLOVERRIDES` entries `<dll-stem>=n,b` (native-first) for every `.dll` in the fixes tree, capped at 1000 chars (launcher.py:249-283); `requires_steam` strips `PROTON_DISABLE_LSTEAMCLIENT` (launcher.py:290-300 approx, `effective_environment`).

Remove-after-play: only the mount and its work/private runtime directory are removed; upper persists ("Only the mount and its work directory are removed after Play", fixes_overlay.py:3-7).

### 3. GoldBerg (gse_fork)

### Asset constants (goldberg.py:38-46)
- `GSE_TAG = "2026_02_16"`, `GSE_ASSET = "emu-win-release.7z"`, `GSE_ASSET_SHA256 = "cdcee5f8a01b837205aba4d0c6867da98975c74d4f767da60b1724c318cdc2e4"`, URL `https://github.com/alex47exe/gse_fork/releases/download/2026_02_16/emu-win-release.7z`. PE machines `_MACHINE_X32 = 0x014C`, `_MACHINE_X64 = 0x8664`.

### Cache layout
- `assets_root = <app_root>/goldberg/gse_fork-2026_02_16` (goldberg.py:53-55). Extraction `_layout` requires, relative to root: `regular/x32/steam_api.dll`, `regular/x64/steam_api64.dll`, `tools/generate_interfaces/generate_interfaces_x32.exe`, `tools/generate_interfaces/generate_interfaces_x64.exe`; optional `steam_settings.EXAMPLE` (goldberg.py:75-88). `_discover_layout` tries the root then, for each `steam_api64.dll` (sorted `rglob`), the base `dll.parent.parent.parent` — i.e. **depth 3** above the DLL (goldberg.py:90-100).
- `ensure_assets`: skip if cached; refuses symlinked cache dir; downloads to `tempfile.mkdtemp(prefix=".gse-", dir=<cache parent>)` with `User-Agent: GameLibraryLauncher/0.2`, 1 MiB chunks, timeout 30, SHA-256 must equal constant else `GoldbergError` (goldberg.py:108-126, 147-200).
- `_extract` uses the 7z helper: `ensure_7z_binary()` (download_service.py:110-137). Helper order: `shutil.which("7zz"|"7z"|"7za")`; else `<$XDG_DATA_HOME|~/.local/share>/umutron/tools/7zz`; else download from **`https://www.7-zip.org/a/7z2603-linux-x64.tar.xz`** (`User-Agent: UmuTron`, `ProxyHandler({})`, extracts member `7zz`, chmod 755). Returns None on failure → `GoldbergError("7-Zip is required...")` (goldberg.py:130-144). Extract command: `7z x -y -o<dest> <archive>` (goldberg.py:139-144).

### PE machine selection (goldberg.py:58-72, 203-273)
- `pe_machine` reads `MZ`, e_lfanew at 0x3C, `PE\0\0` signature, COFF machine at signature+4, raising `GoldbergError` for non-PE.
- Candidate discovery `_candidates`: names `("steam_api64.dll", "steam_api.dll")`; first checks `executable.parent` and game `root`; if none, bounded `os.walk` of root: **depth < 8**, directory names casefolded-skipped `{"content","paks","movies","saved","steam_settings","steamapps","fixes","shadercache","compatdata","__pycache__"}`, stops at first directory containing a name (goldberg.py:203-246).
- `find_steam_api`: computes the game executable's machine (swallow `GoldbergError` → None); preferred = x32 if exe is x32 else x64; picks the first candidate matching preferred, else the first parseable candidate; non-PE candidates are skipped (goldberg.py:247-272).

### Interface generation
- `apply_goldberg` copies the **original** Steam API library and the release generator (`generator_{bitness}`) into staging `<fixes>/.gse-apply-<tmp>/`, chmods the generator 755 (goldberg.py:544-560 approx).
- `_launch_environment` (goldberg.py:296-318): builds env via `launcher.build_command`, runner must exist (`umu-run`); env pops `PROTON_LOG` and `PROTON_LOG_DIR`; if it didn't already exist, the `steam_appid.txt` marker build_command created in the working dir is removed afterward. Invocation: `[runner, generator.name, staged_library.name]` with `cwd=staging`, custom env, `timeout=900` (default). Success requires returncode 0 plus non-empty `staging/steam_interfaces.txt`; failure raises with the last ≤300 chars of stdout (goldberg.py:561-581 approx).

### Install into fixes
- Target path in fixes mirrors the library's relative path under the game root (`fixes / relative_to(root)`); emulator dll copied to staging then `os.replace`d onto target (goldberg.py:527-590 approx).
- `steam_settings/` beside the target DLL receives: `steam_appid.txt` (`<app id>\n`) from `metadata_app_id` or existing `<root>/steam_appid.txt` (must match `[1-9]\d{0,9}`, `umu-` prefix stripped — goldberg.py:284-294), example configs `configs.app.ini`/`configs.main.ini`/`configs.overlay.ini`/`configs.user.ini` copied from `<release>/steam_settings.EXAMPLE/<name>.EXAMPLE.ini` only when absent (goldberg.py:591-605 approx), and `steam_settings/steam_interfaces.txt` via `os.replace` from staging. A stale parent-level `steam_interfaces.txt` is deleted (goldberg.py:603-608 approx).
- Returned dict: `{"fixes","library","settings","interfaces","bitness","environment"}` where `environment = launch_environment(bitness, game["launch"]["environment"])` (goldberg.py:610-618).

### Environment entries (goldberg.py:461-498)
- Set: `PROTON_DISABLE_LSTEAMCLIENT = "1"`; `WINEDLLOVERRIDES += ";<name>=n,b"` where name = `steam_api64` (x64) or `steam_api` (x32); duplicates by name (case-insensitive) avoided; other overrides preserved. Cleaning (`clean_launch_environment`) removes exactly `PROTON_DISABLE_LSTEAMCLIENT == "1"` and override parts whose key casefolds to `steam_api64`/`steam_api`, dropping `WINEDLLOVERRIDES` if empty.

### Detection & reset rules
- Payload locations `_payload_locations`: scan **only** fixes: every `rglob("steam_settings")` dir yields `(parent, settings, parent/steam_interfaces.txt)`; then every `rglob(steam_api*.dll)` file adds its parent if not already present (goldberg.py:350-378).
- `applied`: True if any location has a `steam_settings` dir or a byte-identical release DLL (`_same_bytes`, goldberg.py:383-404, 407-423).
- Reset-on-reapply `_reset_applied(target)` (goldberg.py:320-346): removes `target.parent/steam_settings`, the target DLL itself (not symlink), stale `steam_interfaces.txt`, and any `.gse-apply-*` staging dirs — never other fixes (unpacked exes preserved, goldberg.py:19-21).
- Removal `remove_goldberg` (goldberg.py:425-458): per payload location removes steam_settings dir, interface list, staging dirs, and **each steam_api(64).dll only when byte-identical to the cached release DLL** (`_is_release_library`); user-replaced DLLs survive. No match → `GoldbergError("No GoldBerg files were found in this fixes folder.")`. All touched paths refuse symlinks.

### 4. Steamless

### Asset source & layout (steamless.py:26-61, 99-144)
- `STEAMLESS_URL = "https://codeload.github.com/krunaldodiya/steamless-linux/tar.gz/refs/heads/main"` (no sha256 pin for this asset — `unknown`).
- Root: `<app_root>/steamless/steamless-linux`; `_layout` requires `root/steamless` (wrapper script) and `root/Steamless.CLI.exe`; wrapper chmod 755 after install. `_discover_layout`: direct check, else per sorted `rglob("Steamless.CLI.exe")`, `_layout(candidate.parent)`. Download uses same `User-Agent: GameLibraryLauncher/0.2`, 30 s timeout; tar extraction validates members: rejects absolute paths, `..`, symlinks, hard links; `filter="data"` (steamless.py:85-97).

### Launcher discovery (steamless.py:147-171)
- `launchers(game)`: registry = `game["executable"]` first (primary), then every `launch_targets[i].executable` via `launch_targets.normalize`; each is `resolve(strict=True)`, must be a regular file **inside the game root**, deduplicated. Non-resolvable skipped.

### `.bind` pre-check (steamless.py:174-203)
- `_has_steamstub` parses PE sections; `None` (unknown → Steamless still allowed) for non-PE or unreadable; `0 < section count <= 96`; a section whose first 8 bytes (NUL-stripped, lowercase) equal `.bind` → True.

### Apply flow (steamless.py:325-429)
- Requires `wine` on PATH (`SteamlessError` otherwise). Reset previous first (`_reset_previous`, steamless.py:279-290): for each manifest entry whose file matches its recorded `sha256`, unlink; then delete manifest. Per target, path `fixes / relative_to(root)`:
  - `existing` — destination already in fixes (never overwritten/deleted): result `{"path","outcome":"existing"}`.
  - `unprotected` — `_has_steamstub(source) is False`: skipped, not copied.
  - else copy2 to fixes, run `[wrapper, "--inplace", <destination>]` with `cwd=tool.root`, env `STEAMLESS_HOME=<tool root>`, `WINEDEBUG=-all`, `timeout=600` (default). Non-zero → both destination and `<name>.unpacked.exe` deleted, `SteamlessError` with last ≤300 stdout chars.
  - If `.unpacked.exe` missing → outcome `unprotected` (and the copied destination deleted). Else `os.replace(<name>.unpacked.exe, destination)`; record entry `{"path": "<relative posix>", "sha256": <hex>}`, outcome `unpacked`.
- Manifest written only if any entries: `_write_manifest` atomic via `.tmp` + `os.replace` (steamless.py:228-233).

### Manifest format (steamless.py:27-28, 206-233)
- File `fixes/.umutron-steamless.json`; shape:

```json
{"format": 1, "files": [{"path": "GameDir/launchers/x.exe", "sha256": "<64-hex>"}]}
```
(indent=2). Read validates `format == 1` (MANIFEST_FORMAT) and a list-of-dict `files`, symlink manifest rejected.

- Outcome states: `unpacked` (installed + recorded), `unprotected` (copy skipped or `.unpacked.exe` absent), `existing` (fixes already supplied that path). Return dict `{"fixes","files","skipped","existing","results"}`. `applied()` = any manifest entry resolves inside fixes (steamless.py:265-276).

### Removal / hash rule (steamless.py:292-323)
- `remove_steamless`: each manifest entry's file in fixes is unlinked **only while its SHA-256 equals the recorded hash**; changed files are `kept` (never destroyed) and if *nothing* was removed → `SteamlessError("The Steamless launchers were changed and were not removed.")`. Manifest deleted afterwards.

### 5. Import/export ZIP (fixes_overlay.py:260-470)

- Limits: `_MAX_ARCHIVE_ENTRIES = 100_000`, `_MAX_ARCHIVE_BYTES = 4 GiB`, `_MAX_ARCHIVE_RATIO = 1000` (fixes_overlay.py:260-262). Encrypted entries (`flag_bits & 0x1`) rejected; symlink and non-regular, non-dir entries rejected; zero-file archives rejected; path safety `_archive_parts` rejects NUL, backslash, absolute, `..`, empty parts (fixes_overlay.py:265-274).
- Import flow: staging `tempfile.mkdtemp(prefix=".fixes-import-", dir=fixes.parent)` with `tree/` child; existing fixes are **merged** into the tree unless `replace_existing=True` (fixes_overlay.py:276-340). Existing fixes containing a symlink abort the import first (fixes_overlay.py:296-299).
- Wrapper stripping: if the archive contains no top-level root file and exactly one top-level folder whose name equals the game folder (i.e. a single wrapping folder), that prefix is stripped; other top-level folders (e.g. `bin/`) preserved. Top-level root files disable stripping. Marker files `steamtron-fixes.json`/`umutron-fixes.json` (`FIXES_ARCHIVE_MARKERS`, fixes_overlay.py:23) signal portability and also disable stripping (fixes_overlay.py:349-356).
- Duplicate paths compared casefolded —rejected (fixes_overlay.py:358-362). Extracted file modes: `0o755` if the archive sets any `x` bit else `0o644` (fixes_overlay.py:370).
- Swap: existing fixes renamed to `<fixes>.old-<hex8>`, tree renamed in; on failure old restored; old removed after success; returns `{"path","files","bytes"}` (fixes_overlay.py:374-396).
- Export: `export_name(title)` maps forbidden chars `\/:*?"<>|` and `<ord 32` to space, collapses whitespace, trims `.` and spaces, caps 180 chars, falls back `Game`, suffix `.zip` (fixes_overlay.py:399-404). ZIP contains marker first as `steamtron-fixes.json` with content `{"format":1}\n`, then each regular file at its relative posix path; symlinks abort; temp-file + `os.replace` atomic; existing ZIP replaced (fixes_overlay.py:406-470).

### 6. Porting notes and traps

- Fixed layer order is always **fixes above game root** in `lowerdir` (fuse-overlayfs treats the first entry as top layer): `lowerdir=<fixes>:<root>` (fixes_overlay.py:621) — Rust must preserve this ordering or fixes will lose DLL shadowing.
- Upper persistence is squeezed into the "no game writes touch the install" contract: work dirs are transient, upper dirs are keyed by a stable `sha256[:32]` of game identity and must survive cleanup (fixes_overlay.py:566-570, 681-688). Identity hierarchy matters: `id` → `metadata_app_id` → root.
- Runtime dirs must not live under `XDG_RUNTIME_DIR` (pressure-vessel remaps it); TMPDIR must be absolute; `/tmp` then `/var/tmp` fallback; uid-suffixed per-user base (fixes_overlay.py:554-573).
- Ownership checks (uid) on every overlay dir plus chmod 0700, and the "private layer" proof relies on the write probe appearing in the upper dir, not just on a successful write (fixes_overlay.py:715-775).
- Goldberg's interface generation deliberately suppresses `PROTON_LOG` and removes a transient `steam_appid.txt` marker; the launch-supervisor's own `steam_appid.txt` creation only happens when `overlay is None` (nowhere — fixed. Quoted from launcher.py:739-742 and steamless.py:148-171). Interface generation requires the *original* valve library bytes at generation time; reapply always re-resets (goldberg.py:511-515).
- Hash-based removal symmetry: removal is **exact-release-identity** conservative. Goldberg deletes a DLL only if byte-identical to the cached release (goldberg.py:425-458); Steamless only if sha256 matches its manifest, else kept, and error when nothing removed — repeated applications must regenerate manifests (steamless.py:292-323).
- Steamless raise behavior: returncode 0 but no `.unpacked.exe` is a *soft* skip (`unprotected`), while non-zero returncode is a *hard* error with output tail; the errors surface the last ≤300 whitespace-joined stdout chars in both Goldberg and Steamless (goldberg.py:581; steamless.py:386-396 approx).
- Steamless wrapper env: `STEAMLESS_HOME` must point at the tool root and `WINEDEBUG=-all`; the tool itself is a Wine CLI (`Steamless.CLI.exe`) driven by a Linux `steamless` wrapper script requiring `wine` on PATH (steamless.py:40-45, 334, 372-384).
- Import/export strip logic: "wrap in game-named folder" vs "foreign top-level folder" is name-sensitive (`top == root.name`) and marker-sensitive; ZIP duplicates are compared **casefolded**, so case-only differing paths from Windows zips are refused — replicate exactly (fixes_overlay.py:349-362).
- The 7z bootstrap Pin: `7z2603-linux-x64.tar.xz` with member filename `7zz` (download_service.py:123-132). System binary discovery order `7zz,7z,7za` precedes the download; a missing XDG home dir is created at `<tools>/7zz` (download_service.py:114-122).
- Every ZIP/tar extractor validates path members and refuses symlinks/hard links; fixes consumer paths re-check symlinks repeatedly (e.g., `parent`, `fixes`, target, manifest) — do not collapse these into one check in Rust since the Python semantics re-validate at each mutation point.
- Export of ar archives: root file vs single top-level folder ambiguity — root file present means "no wrapping folder" and could mis-handle zips with one top-level dir plus one root file: in that case `root_files` is true, strip is false, preserve everything (fixes_overlay.py:349-356).
- `fixes_enabled` absent=true is a compat trap: an old record without the key launches with fixes on (fixes_overlay.py:47-57).
- JSON manifest shapes (constants): Steamless manifest `{"format":1,"files":[{"path","sha256"}]}` (steamless.py:27-28, 228-233). Fixes export marker content is exactly `'{"format":1}\n'` written as `steamtron-fixes.json` (fixes_overlay.py:419 — `bundle.writestr`). App manifest `fixes_directory` is always `"fixes/<folder>"` (game_assets.py:216, 261). Backup components include `fixes.tar` when the game has fixes plus `fixes_enabled` setting, restored to `<umuapps>/fixes/<folder>` with record key `fixes_path` (game_assets.py:894-929, 1231-1280).
- `fixes_dll_overrides` caps the `WINEDLLOVERRIDES` string at 1000 chars, and derives names from every `.dll` recursively in fixes — implementers must reproduce the cap and the `n,b` native-first flag (launcher.py:249-283). `clean_launch_environment` is value-exact for `PROTON_DISABLE_LSTEAMCLIENT` and key-exact (`steam_api64`/`steam_api`) for overrides (goldberg.py:461-498).
- Unknown: Steamless asset is a moving `main`-branch tarball with no sha256 pin; pinned revision of `steamless-linux` itself is `unknown`. GSE example config inner names (`configs.*.EXAMPLE.ini`) come from the fixed release tag (goldberg.py:597-605).

---

## Library, UI, and interaction

### 1. Library model (`umutron/library.py`)

- Module scope: `"Local library and portable backups. This module never launches games."` `umutron/library.py:1`.
- **Document shape**: `library.json` is `{"version": 1, "games": [], "settings": {"theme": "system"}}` — `umutron/library.py:198-202`.
- **Settings fields** (validated in `Library._validate`, `umutron/library.py:330-389`):
  - `theme` / `theme_id`: `"system"|"light"|"dark"` or custom id matching `[A-Za-z0-9][A-Za-z0-9._-]{0,127}` (`_THEME_ID`, `umutron/library.py:42-45`; checked `umutron/library.py:341-346`).
  - `default_display_mode`: `"desktop"|"fullscreen"` — `umutron/library.py:348-350`.
  - `ui_scale`: `"auto"|"100"|"125"|"150"|"175"|"200"` — `umutron/library.py:352-353`.
  - `gamemode`: bool — `umutron/library.py:355-356`.
  - `picker_folders`: dict, ≤64 entries; keys `[a-z0-9_]{1,40}`, values absolute paths ≤4096 chars without NUL — `umutron/library.py:358-366`; write-side `remember_picker_folder` enforces the key regex, absolute+existing dir, and evicts alphabetically-first keys beyond 64 — `umutron/library.py:497-512`.
  - `default_proton`: str ≤4096, no NUL; setter additionally parses the release via `runner_selection.parse_release` and rejects `INHERIT` (`"Choose a concrete app default or automatic policy."`) — `umutron/library.py:376-379,466-473`.
  - `recent_viewed`: list ≤8 rows (`MAX_RECENT_VIEWED = 8`, `umutron/library.py:40`); each row keys exactly `{id,name,steam_app_id,game_type}`, `id` int>0, name non-blank ≤1000 — `umutron/library.py:381-391`; readers/writers `recent_viewed()` `umutron/library.py:514-517`, `remember_viewed()` `umutron/library.py:519-541`, `clear_recent_viewed()` `umutron/library.py:544-546`.
- **Per-game record** (`new_game`, `umutron/library.py:394-410`): `id` (UUID string), `metadata_app_id`, `metadata_source={}`, `artwork={}`, `sync=None`, `launch={"proton": DEFAULT_GAME_PROTON}`, `installation={}`, and empty text fields `title, executable, working_dir, arguments, description, release_date, developers, publishers, genres, uninstaller` (`TEXT_FIELDS`, `umutron/library.py:26-36`). Deep copies, never live references: `games()` — `umutron/library.py:406-407`.
- **Validation rules** (`validate_game`, `umutron/library.py:149-191`):
  - `metadata_source`: dict with only `provider,id`; provider `steam|igdb`; `id` int>0 — `umutron/library.py:152-161`.
  - Text fields: str, ≤100000 chars, no NUL — `umutron/library.py:162`.
  - `metadata_app_id`: optional int>0 — `umutron/library.py:163-165`.
  - `artwork`: dict whose keys are a subset of `ART_KINDS = ("portrait","landscape","hero","logo","icon")` (`umutron/library.py:25`); each value `[a-f0-9]{64}\.(png|jpg)` — `umutron/library.py:178-182`.
  - `sync`: optional; str `account`, int `shortcut_id` 0..0xFFFFFFFF, str `digest` — `umutron/library.py:184-191`.
  - `quicklook` keys `size_bytes,size_path,size_measured_at,proton` (+ ProtonDB subkeys `tier,bestReportedTier,confidence,total,fetched_at`, strings ≤64, ints ≥0) — `umutron/library.py:108-146`.
  - Delegates to `launcher.validate_settings`, `installations.validate_installation`, `installer_identity.validate_installer_attempt`, `setup_history.validate_timestamp`, `updates.validate_updates`, `fixes_overlay.fixes_enabled`, `launch_targets.validate_targets` — `umutron/library.py:154-177`.
- **Atomic writes**: `atomic_write` creates a hidden temp file in the destination's parent, writes, flushes, `fsync`s, then `os.replace`; leftover temps removed — `umutron/library.py:49-63`. `_write` = pretty `indent=2, ensure_ascii=False` JSON via `atomic_write` — `umutron/library.py:391-392`.
- **Save / delete with rollback**: `save` validates, requires non-blank title (`"Enter a game title."`), mirrors quicklook into the on-disk manifest (must never fail the save), snapshots previous data, replaces or appends by `id`, and restores the snapshot on write error — `umutron/library.py:412-431`. `save_setup` merges setup history — `umutron/library.py:433-439`. `delete` snapshots and filters by id with rollback — `umutron/library.py:441-448`.
- **Sync digest/status**: `digest` = sha256 of the game minus `sync`/`launch` with sorted JSON keys — `umutron/library.py:103-105`. Display strings: `"Not synced"`, `"Synced"`, `"Changes pending"` — `umutron/library.py:562-565`; `mark_synced`/`mark_unsynced` — `umutron/library.py:548-560`.
- **Migration / strip rules**:
  - `game-library-launcher` → `umutron` data-home rename, copying artwork + library.json — `umutron/library.py:186-241`.
  - `steam-library-metadata-manager` legacy import: revalidates, copies only artwork whose sha256+ext matches its filename, otherwise `"Legacy artwork is invalid."` — `umutron/library.py:259-273`.
  - Legacy fixes adoption: `<app data>/fixes/<App ID>` folders moved into `umuapps/fixes/<Game>`, only when the canonical target doesn't exist (never silently merged); removes `fixes_path` from games — `umutron/library.py:275-309`.
  - Size guards: library.json >20 MB rejected — `umutron/library.py:220-221`.
- **Artwork**: `add_image` — `"Artwork exceeds 20 MB."`, name = sha256+ext — `umutron/library.py:567-573`; PNG/JPEG sniffing and dimension caps (8192 per side / 32 MP) in `image_extension` — `umutron/library.py:64-101`.
- **Portable backup zipper** (used by backup UI): `export_zip` strips `sync` (→ null) and keeps only `theme, default_proton ("UMU-Latest"), default_display_mode ("desktop"), ui_scale ("auto"), gamemode (True)` — `umutron/library.py:575-592`; `_read_archive` rejects >20000 entries/>512 MB (`MAX_ARCHIVE`, `umotron/library.py:39`), unsafe entries, unknown names, artwork checksum mismatch — `umutron/library.py:599-631`; `preview_import` returns counts and titles — `umutron/library.py:633-641`; `import_zip` supports conflict policy `keep|replace` (`"Select a conflict policy."`) — `umutron/library.py:643-665`.

### 2. Screens and routes

- **Route names**: `"library"`, `"store"` (registered route states, `umutron/catalog_ui.py:206-214`), plus `"home"` (`umutron/catalog_view.py:30-31`) and `"detail"` (`umutron/catalog_ui.py:1068-1069`). State per route: `query, page, genre, sort (0 for library / "popular" for store), scroll, focus` — `umutron/catalog_ui.py:206-214`. `begin_route` resets the body, play buttons and nav-button highlight; `heading` = `UmuTron` / `Your games · UMU and Proton` — `umutron/catalog_ui.py:290-322`.
- **Desktop header nav**: buttons `Home`(`show_home`), `Library`(`navigate_library`), `Store`(`navigate_store`) — `umutron/app.py:221-231`; `Back` button — `umutron/app.py:218-219`; plus Settings/account/add-menu icon buttons — `umutron/app.py:239-267,301-303`.
- **TV (console) header**: `Backing out`→`Back`, brand, `Home`/`Library`/`Store` flat buttons, desktop-toggle `Switch to desktop mode (F11)`, `Fullscreen options`, clock `tv-clock` (updates every 30s) — `umutron/app.py:279-303_321-339` (tv controls `umutron/app.py:279-303`, clock `umutron/app.py:303-332`).
- **Home screen**: hero "RECENTLY PLAYED" is a big featured selection with status bar; sections `"Recently played"` (H-scroll rail) and `"Recently ready to play"` (grid-ish rail). Buttons `Play` & `View game` — `umutron/catalog_view.py:100-141`; headings/empty copy `umutron/catalog_view.py:148-183,196-219`; setup tile statuses: `Ready to play`, `Files unavailable`, `Setup needed` (`umutron/catalog_view.py:203-217`). Empty-library StatusPage: `Your next adventure starts here` — `umutron/catalog_view.py:49-53`.
- **Library screen**: heading `Your library` (`umutron/catalog_ui.py:330-340`), search `Search your library` + `Search` button (`umutron/catalog_ui.py:348-352,357-360`), Gtk.FlowBox grid of `.collection-card` tiles (cover art + 2-line wrap caption + optional badge + state icon overlay) — `umutron/catalog_ui.py:368-372,601-680`. Status text `"{count} games"` (and `"· {total} in your library"` with filters) — `umutron/catalog_ui.py:737-741`. Pager: `First, Previous, [<number> …], Next, Last` — `umutron/catalog_ui.py:516-540`. Page 24 per library card, 24 items/page total (`PAGE_SIZE = 24`, `umutron/catalog.py:30`); store max page `MAX_PAGE = 1000`.
- **Store screen**: heading `Discover your next game` — `umutron/catalog_ui.py:327-347`; category performers such as `Recently viewed` strip (`umutron/catalog_ui.py:813-831`); genre `BrowseChoice("Filter", [...])` — `umutron/catalog_ui.py:762-765`; store card sub-status badges like `"In library"` — `umotron/catalog_ui.py:909-921`.
- **Detail screen** (`show_shared_detail`, `umotron/UI...`): artwork-led surface with title, description, action row, quick-fact chips (IGDB/Steam/ProtonDB/size) — `umotron/detail...`; `saved_detail()` marks whether the item is a library entry — `umotron/catalog_ui.py:1502-1504`; primary action gates on background detail fact lookups (`begin_detail_task/finish_detail_task/settle_detail_gate` — `umotron/catalog_ui.py:1592-1611`). Store/ignore copy to/from saved entries is prevented (`open_related_entry`) — `umotron/catalog_ui.py:1506-1515`.
- **Game Settings dialog tabs** (`open_game_settings`, built in a maximized modal, `umotron/detail_view.py:256-309`): it builds the fixed pages in order and opens at a chosen page (`page="general"` default) — `umotron/detail_view.py:256-305`. Tabs:
  1. `General` — General page title `"General"` (`umotron/detail_view.py:312`); owns pre-launch script approval (Hash SHA-256 approval via `prelaunch.approve`, `umotron/detail_view.py:330-380`), performance switches MangoHud and Gamescope&FSR, and the `Save general settings` action — `umotron/detail_view.py:391-427`.
  2. `Fixes` — import/export/enable per-game fixes; never tweaks installed files or prefix — `umotron/detail_view.py:429-960`.
  3. `Compatibility` — `umotron/detail_view.py:962-1152`.
  4. `Environment` — `umotron/detail_view.py:1154-1235`.
  5. `Debug` — `umotron/detail_view.py:1237-1342`.
  6. `Game Link` — link an unlinked library entry to a folder (`umotron/detail_view.py:127-253,1344-1709`).
  7. `DLC` — added only when the game's source catalog has add-ons — `umotron/detail_view.py:1896-2120`.
  8. `Updates` — `umotron/detail_view.py:1711-1894`.
  9. `Installed Files` — `umotron/detail_view.py:2122-2320`.
- **Global Settings pages** `open_settings(section=None)` (`umotron/settings_view.py:20-273`): `General` page (Display mode row `"Default launch mode"` with `Desktop`/`Fullscreen / TV` strings, `umotron/settings_view.py:60-77`; Feral GameMode row with install story `umotron/settings_view.py:79-118`; `Controller` group with row `Gamepad navigation` and a `Reconnect` button `umotron/settings_view.py:132-141`; app-version group (`APP_NAME + " " + __version__`); `Appearance` group containing a Theme dropdown (`umotron/settings_view.py:157-172`), `Custom Themes` row (drop custom folders in the `custom_dir`) `umotron/settings_view.py:173-180`, and a `UI Scale` dropdown with the six scale strings `umotron/settings_view.py:226-253`; and `Diagnostic logs` group with `Activity` and `Clear all diagnostic logs` buttons — `umotron/settings_view.py:221-233`.
- **Storage page** (`umotron/storage_ui.py:212-256`, `umotron/settings_view.py:231-243`): `Settings → Storage` top-level with page title `Storage` and icon `drive-harddisk-symbolic`; group `Your game drives`, description `Added drives are the only destinations for managed games...`; rows of drive cards plus a `Game backups` group with a `Restore…` action — `umotron/storage_ui.py:219-253`.
- **TV / fullscreen differences**: `set_tv_mode(True)` — `umotron/app.py:359-416`: removes `desktop-mode`, adds `tv-mode`, hides the desktop header (`umotron/app.py:388`), shows `tv_controls`, forces dark color scheme (`umotron/app.py:401`), un-decorates the window, and re-renders the current route. Reentry blocked while an editor or busy-job is active (`umotron/app.py:361-371`), with the toast `Save or cancel the current dialog before switching display mode.` — `umotron/app.py:372-373`. Settings is TV-locked (`umotron/settings_view.py:23-25` "Switch to desktop mode to manage settings."), and game-link/installer editing is desktop-only too (`umotron/detail_view.py:120-122` "Switch to desktop mode to link an installed game.") — the pattern is to raise a toast and do nothing on TV-mode pages.

### 3. Navigation model

- **Keyboard/controller shared dispatch**: pressed key/button results always flow through `OperationController.on_key` (captured `key-pressed` controller registered at `umotron/app.py:330-337`; handler at `umotron/operation_controller.py:210-311`). It maps:
  - `F11` (no modifier, main window, no modal) — switch to fullscreen/TV, or open `Fullscreen options` when already fullscreen — `umotron/operation_controller.py:246-265`.
  - Arrows (Left/Right/Up/Down) — move focus.
  - `Return / KP_Enter / space` — activate the focused control (`select` action) — `umotron/operation_controller.py:202-208,228-239`.
  - `Escape / Back / handle_back` — close window/return from detail — `umotron/operation_controller.py:101-113,238-244`.
  - `PageUp / PageDown` (main window, no modal focus, library/store routes) — page/column — `umotron/operation_controller.py:267-273`.
  - `Tab` / `Shift-Tab` — cycle between next/previous control — `umotron/operation_controller.py:213-244`.
  - In TV, text entries/textviews forfeit arrow keys but still accept Escape — `umotron/operation_controller.py:277-279`; no `Escape` on detail when univ (`umotron/operation_controller.py:274-276`).
- **Controller mappings**: SDL2 pad buttons → actions: `0→"select"`, `1→"back"`, `2→"play"`, `6→"desktop"`, `9→"pageup"`, `10→"pagedown"`, `11..14→up/down/left/right` (`umotron/controller.py:9-21`). Analog stick directions are added as `up/down/left/right` when the value exceeds 16000 (`umotron/controller.py:176-179`). `left/right/up/down` repeat; buttons don't.
- **Focus rules**:
  - `focused_control` counts only `Gtk.Button | Gtk.MenuButton | Gtk.CheckButton | Gtk.SearchEntry` — `umotron/app.py:722-731`; other widgets (labels, pictures, containers) never take focus.
  - `restrict_tv_focus` strips focusability from non-interactive widgets (`umotron/app.py:715-724`).
  - Breadcrumb-style key handling captures focus from within a nested focus chain: `native_key_context` yields keys from text entries, dropdowns, popovers, and modals (`umotron/app.py:1177-1205`); this ensures the app's global key handler doesn't intercept a text field's native typing.
- **Repeat / debounce**: new direction press repeats after `+0.4 s`, then repeats every `+0.16 s` — `umotron/controller.py:47-56`; non-directional buttons never repeat; disabled mode still applies a `+0.4 s` debounce before an action — `umotron/controller.py:51-52`.
- **Controller lifecycle**: SDL2 loaded once; if a game or overlay steals focus, the app calls `controller.reconnect()` (via `on_window_active`, `umotron/app.py:1096-1100`) to release and re-open the device (`umotron/controller.py:192-198`). Device identity relies on `SDL_JoystickPath` st_dev/st_ino to detect silent Linux re-numberings (`umotron/controller.py:23-27,124-127,145-159`). Reconnect swallows the currently-held press via `suppress_next` so it doesn't re-fire an action — `umotron/controller.py:181-197`.
- **Gating**: `controller_enabled` requires `not self.busy`, a visible+active launcher-owned `navigation_window()`, and that the route is in `("home","library","store","detail")` (TV always allowed) — `umotron/app.py:1102-1111`. `frozen` mode (a game overlay opening) = no dispatch reactions — `umotron/app.py:1105-1117`.
- **navigation window**: `navigation_window()` returns the topmost visible launcher-owned window so an owned modal may accept controller navigation itself (`umotron/app.py:1086-1095`); also `on_window_active` reconnects the controller after a game/overlay returns focus (`umotron/app.py:1096-1100`).

### 4. File pickers

- `choose_file(title, callback, save=False, folder=False, filename=None, start_folder=None, parent=None, launcher=False, key=None)` — `umotron/app.py:1416-1429`, `file chooser` created at `umotron/app.py:1430-1441`.
- **Remembered folders per action** (stored as `picker_folders[key]` in library settings, `umotron/library.py:497-512`):
  - `backup_destination` — backup dialog (`umotron/backup_controller.py:54`).
  - `backup_source` — restore dialog (`umotron/backup_controller.py:290`).
  - `manifest_file` — Steam manifest picker (`umotron/catalog_ui.py:2208`).
  - `install_folder` — game install folder (`umotron/catalog_ui.py:3728`).
  - `fixes_folder`, `fixes_import`, `fixes_export`, `linked_game_folder` — `umotron/detail_view.py:653,686,711,1387`.
  - `working_dir`, `prefix_executable`, `saves_export`, `saves_import`, `installer` — `umotron/app.py:2449,2780,2155,2204,2222` and `umotron/install_controller.py:147` (`working_dir`).
- **Start folder precedence** — `umotron/app.py:1471-1504`, applied in this order, first successful wins:
  1. explicit `start_folder` (`umotron/app.py:1471-1476`);
  2. remembered by `key` (`umotron/app.py:1477-1482`);
  3. `fields["working_dir"]` / `fields["executable"]` parent dir (`umotron/app.py:1483-1496`);
  4. `default_install` from `storage_service.snapshot()` (`umotron/app.py:1497-1507`);
  5. `Path.home()` (`umotron/app.py:1508`).
  - On a successful pick with a `key`, `remember_picker_folder` is called with the chosen directory (or the file's parent) — `umotron/app.py:1515-1530`; the callback errors propagate to `self.error(...)` — `umotron/app.py:1531-1532`.
- **What is stored**: an absolute directory path string; at most 64 keys (alphabetical eviction past 64 — `umotron/library.py:500-512`); tool validates the key shape and path pattern — `umotron/library.py:497-512`.

### 5. Error / confirmation patterns

- **Toast**: `self.notify(text)` — logs to the Activity pane and posts an `Adw.Toast` — `umotron/app.py:1253-1260`. It's the lightweight, non-modal channel for informational or recoverable-flow outcomes (`umotron/app.py:1253-1260`).
- **Error dialog**: `self.error(error, parent=None)` opens a modal `message_dialog` titled `"Could not complete this action"` with the stringified error (truncated to 2000 chars) — `umotron/app.py:1262-1273`. It prefers a modal parent (archive dialog → game settings dialog → installer dialog → editor → main window). Activity log is stored to `[last 100 lines]` — `umotron/app.py:1246-1250`; reveals via `Activity` button — `umotron/app.py:1275-1276`.
- **Confirmations**: `self.confirm(title, body, action, callback, destructive=False, parent=None, cancel=None, default="cancel")` — `umotron/app.py:1387-1389`; always builds two responses `("cancel","Cancel")` and `("confirm", action)` — `umotron/app.py:1392-1399`; the destructive flag renders the confirm row red, not as the default — `umotron/app.py:1387-1414`.
- Any **long-running work** has a persistent inline progress strip, not a toast: `self.async_job(message, work, done, progress=None, main_progress=True)` — `umotron/app.py:1302-1349`; a modal strip with a spinner + optional fraction bar, leaves the window hover-sensitive but disables body/header sensitivity while running — `umotron/app.py:1312-1348`.
- Archive/backup long jobs use a separate `archive_job(dialog, message, work, done, on_error=None)` with a cooperative `cancelled` flag and a dedicated `archive_dialog` (backup/restore dialog owns its own progress) — `umotron/app.py:1350-1386`.
- **Inline field errors**: the storage pages and pickers set text visible within the page (`show_error`) — `umotron/storage_ui.py:276-280,187-191`; settings and storage pages use inline strings, and only medium-certain flows commit a native `confirm` (destructive removals). Examples of destructive confirmations:
  - Uninstall review: modal title `Uninstall {game["title"]}?` with the body `Only the game content is deleted. The library entry, app manifest, generated .lua, Proton prefix/runtime and fixes are kept; delete them manually if required.` and button `Delete game folder` — `umotron/catalog_ui.py:3628-3701`.
  - Library removal: `Remove {title} from library?` with `Only its library entry is removed; you stay on this page. Game files, app manifest, .lua, saves, prefixes and Proton are kept.` and confirm string `Remove from library` — `umotron/catalog_ui.py:3577-3626`.
  - Storage drive remove: `Remove this storage registration?` — only the registration is removed; the game files stay on the drive — `umotron/storage_ui.py:293-318`.
  - Unused fixes delete: `Delete unused fixes?` — `umotron/storage_ui.py:320-352`.

### 6. Backups/restore and storage flows

- **Backup dialog** (`umotron/backup_controller.py:21-231`): outline `Back Up Game` / `Create a portable content backup.` — `umotron/backup_controller.py:31-40`; destination pick uses `folder=True, key="backup_destination"` — `umotron/backup_controller.py:52-55`; text `Creates content, game fixes, cached download manifests, launcher settings, and compatibility choices.` — `umotron/backup_controller.py:57-59`; confirm button `Back up game`.
  - overwrite protection: when the destination already exists, a `confirm` with `Overwrite existing backup?` / `Overwrite backup` is required — `umotron/backup_controller.py:107-125`.
  - cancel protection: confirms `Cancel backup?` / `Cancel backup` before aborting the job — `umotron/backup_controller.py:78-105`; a second press while pending is blocked.
  - Live statistics (speed/elapsed/ETA, GiB-of-GiB bar) are updated from the work thread — `umotron/backup_controller.py:137-193`.
- **Restore dialog** (`umotron/backup_controller.py:233-490`): headline `Restore Game Backup`, subtitle `Restore this backup into the selected UmuLibrary common directory.` — `umotron/backup_controller.py:247-254`; destination requires at least one registered install drive; otherwise toast `Add a game drive in Settings → Storage first.` — `umotron/backup_controller.py:255-262`. Backup folder via `folder=True, key="backup_source"`, and the restore outcome is pre-selected with either `Its existing library entry will be updated.` or `It will be added to your library.` — `umotron/backup_controller.py:274-293`. Drive dropdown choices are displayed as `"label · root"` + ` · Default` — `umotron/backup_controller.py:370-373`. Overwrite existing folder needs `Overwrite installed files?` / `Overwrite files` confirmation — `umotron/backup_controller.py:323-350`; cancel protection mirrors the backup dialog — `umotron/backup_controller.py:290-315`. After a successful restore, the library saves a linked entry (`_save_managed_game`) and notifies `Restored {title} successfully.` — `umotron/backup_controller.py:474-489`.
- Both flows always check for in-flight archives and refuse/reopen the running dialog (`archive_future` guard — `umotron/backup_controller.py:23-28,235-240`; `archive_job` guard `umotron/app.py:1351-1356`).
- **Storage management** (`umotron/storage_ui.py`):
  - Registering a new game drive via `VolumePicker` — `umotron/storage_ui.py:80-208`: title `Add game drive`; copy states registration alone creates nothing (text `Registration keeps existing game locations unchanged. No partition, folder or game file is created.`); radio-selection of registered/unregistered internal volumes, each listing free space and a validation `reason`; `Refresh drives` button; `Add drive` commits.
  - Registered drive cards show roles, capacity and `Connected · Writable for managed games` (or a `Managed game storage unavailable: …` warning) — `umotron/storage_ui.py:354-426`.
  - One volume can become default (`Use for games`) — `umotron/storage_ui.py:390-402`; only one drive is the install default.
  - Remove needs the destructive confirm above, then `service.remove(row_id)`.
  - Registration never de-locates games — `Nothing is deleted; adding a drive is metadata only.` is stated in `umotron/storage_ui.py:220-224,431-434`. This makes drive roles and removal separate from game data.
  - The install-drive chooser dialog (`InstallDriveDialog`, `umotron/storage_ui.py:493-640`) reviews the download (`Archive`) vs install (`Expanded`) budgets independently for the cache and install drives, printing strings like `Isolated preview: this selects a target only. It cannot start a download or write game files.`, and only enables `Use this drive` when a live `quote` says everything fits — `umotron/storage_ui.py:537-587,631-640`.

### 7. Porting notes

The Tauri UI is a Rust rewrite; the behavior to reproduce (preserving all semantics):
- **Library**: implement the same JSON document shape and validation (now in Rust): the 10-char settings whitelist (`theme`, `theme_id`, `default_display_mode`, `ui_scale`, `gamemode`, `picker_folders`, `default_proton`, `recent_viewed`), the game record schema (10 text fields, `metadata_source`, `artwork` keyed by the 5 `ART_KINDS` with `[a-f0-9]{64}.png|jpg` names, `sync`, `quicklook`), library the 10000-game cap and the 20 MB library.json size guard — `umotron/library.py:330-391`.
- **Atomic writes**: replace `tempfile.mkstemp`/`os.replace` with the same write-then-rename pattern in Rust (std `std::fs`, or `tempfile` crate) — requirements identical to `umotron/library.py:49-63`.
- **Routes/screen layout**: `home | library | store | detail` with per-route remembered `query/page/sort/scroll/focus`, store genre `"popular"`, and 24 items per page — `umotron/catalog_ui.py:206-214`. Library pager limit `MAX_PAGE=1000` for store searches ("Narrow your search to see more games.") — `umotron/catalog.py:30-31,177-180`; `recently-viewed` strip per store tab (`umotron/library.py:514-546`).
- **Navigation**: re-implement the SDL2 gamepad listener and the keyboard handler as a Tauri plugin but map the exact same action set (select/back/play/desktop/pageup/pagedown/dpad), debouncements (`0.4 s` new, `0.16 s` repeat direction-only), and gating rules (`busy`, `navigation_window`, and the `("home","library","store","detail")` whitelist) — `umotron/controller.py:9-21,40-56` and `umotron/app.py:1086-1111`.
- **File pickers**: preserved start-folder precedence (explicit `start_folder` → remembered `picker_folders[key]` → working dir/executable → default storage → `$HOME`) and the same stored `picker_folders` map with the `[a-z0-9_]{1,40}` keys — `umotron/app.py:1471-1504`, `umotron/library.py:497-512`.
- **Errors / progress**: replace toasts with a native Tauri toast (persistent, not modal) and inline error strings on storage and settings pages; destructive actions must still be confirmed with the same wording — `umotron/app.py:1253-1276,1387-1414`.
- **Backups and storage**: the backup/restore dialogs must keep the same roles (`Back Up Game`, `Restore Game Backup`), the overwrite (`Overwrite existing backup?` / `Overwrite installed files?`) and cancel protections, the GiB-of-GiB + speed/ETA progress strings, and the quote-based install-drive choice (both cache and install budgets must fit) — `umotron/backup_controller.py:21-490` and `umotron/storage_ui.py:493-640`.

**GTK-specific details to replace:**
- Adw.HeaderBar / Adw.PreferencesWindow — Tauri uses HTML-webview native windows. Everything described above in `umotron/app.py:117-357` is window chrome, port what's user-visible (HeaderBar row layout, title/masthead, buttons) into Tauri UI elements.
- `Adw.Toast`/`Adw.ToastOverlay` — native window overlay; replace with a webview element (positioned fixed at a top/bottom overlay) — `umotron/app.py:171-173,1258-1260`.
- `Gtk.FileChooserNative` with `FileFilter` entries for game launchers / archives — `umotron/app.py:1429-1470` — replace with OS-native picker (Tauri's `tauri-plugin-dialog`), honoring the same `launcher` filter patterns (`*.exe`, `*.AppImage`, `*.sh`, `*.run`, `*.bin` and MIME types) — `umotron/app.py:1443-1465`.
- GTK `EventControllerKey`, `Gtk.GestureClick`, focus chain, `Gtk.FlowBox` grid — replace with DOM focus management and JS `keydown` handlers; replicate `focused_control` semantics (only button/menu/check/search controls participate) — `umotron/app.py:722-731`.
- `GLib.idle_add` / `GLib.timeout_add` / `ThreadPoolExecutor` — Tauri async runtime with a dedicated thread pool; keep the 40 ms poll cadence (`umotron/app.py:339`) and the 30 s clock tick (`umotron/app.py:302-303`), and preserve the fact that a block for one long job disables the window body/header (`umotron/app.py:1311-1349`).
- `Adw.StyleManager` and theme classes (`desktop-mode`/`desktop-dark`/`tv-mode`/`scale-*` CSS classes, `umotron/app.py:1278-1300` and `umotron/catalog_ui.py:102-133`) — use CSS variable/theming in the webview driven by the `theme_id` custom themes from `umotron/theme_engine.py`.
