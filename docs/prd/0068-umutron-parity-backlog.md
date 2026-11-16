# UmuTron feature parity backlog

- Status: draft
- Date: 2026-11-16
- Areas: `crates/steamtron-desktop` (Rust commands and `ui/app.js`), `crates/steamtron-core`, `crates/steamtron-library`, `crates/steamtron-auth`, packaging and documentation

## Problem

SteamTron and UmuTron overlap on the media-library shell, but SteamTron is
missing several UmuTron subsystems and has weaker lifecycle/safety behavior in
others. The gap is not a single bug. It spans installer execution, repack
payload handling, release-source discovery, managed-library curation, themes,
single-instance behavior, guarded removal, process supervision, native Linux
launch, and several smaller parity items.

This PRD records the complete feature audit so the team can select a subset to
implement later. It is a backlog and decision record, not an implementation
plan approved as a whole.

The comparison was made against:

- SteamTron `main` at `f1f0d77`.
- UmuTron `main` at `4b60545` in
  `/mnt/krunal/WorkSpace/Code/UMUTron-WorkSpace/UmuTron`.
- UmuTron’s `README.md`, `docs/DESIGN.md`, `docs/STORAGE.md`,
  `docs/AUTOMATED-INSTALLATION-PIPELINE.md`, `docs/DOWNLOAD-SERVICE-CONTRACT.md`,
  and the modules cited below.
- SteamTron’s existing `docs/UMUTRON-BEHAVIOR-REFERENCE.md` and the navigation,
  DLC, cache, backup, and runtime PRDs linked at the end.

Status legend used in the tables:

- `parity`: SteamTron already has the behavior.
- `partial`: a subset exists, or the behavior exists with a materially narrower
  contract.
- `gap`: no equivalent behavior was found.
- `ahead`: SteamTron is stronger than UmuTron.
- `not a gap`: UmuTron contains code or documentation for the behavior, but it
  is not exposed or not part of the current product.

Priority legend:

- `P0`: largest product or safety gap; decide early.
- `P1`: important parity or lifecycle gap.
- `P2`: meaningful but narrower user impact.
- `P3`: polish or low-frequency workflow.
- `—`: informational only, not an implementation candidate.

## Goal

Provide a single, itemized reference that lets the team:

1. Compare UmuTron and SteamTron feature by feature.
2. Select explicit backlog IDs for future implementation.
3. Reject intentionally out-of-scope items without re-litigating the audit.
4. Preserve SteamTron’s existing Steam-specific advantages while closing chosen
   UmuTron gaps.
5. Avoid porting dead, removed, or merely documented UmuTron behavior.

## Non-goals

- This PRD does **not** approve implementation of any item.
- This PRD does **not** select a subset. Selection is deferred.
- This PRD does **not** define implementation order beyond the priority hints.
- This PRD does **not** change SteamTron behavior, source code, UI, tests,
  packaging, or installed binaries.
- This PRD does **not** make UmuTron the product source of truth. SteamTron’s
  account model, Steam registration, SLSsteam, fixes tooling, Windows support,
  and Doctor remain product decisions.
- This PRD does **not** require parity for features that conflict with
  SteamTron’s product direction.
- This PRD does **not** copy UmuTron’s GTK-specific implementation. Only
  user-visible behavior and safety contracts are relevant.

## Feature inventory

### Catalog and library

| ID | Pri | Feature | UmuTron reference | SteamTron today | Status |
|---|---|---|---|---|---|
| L-01 | — | Home hero, Recently played rail, Recently ready to play rail | `umutron/catalog_view.py`, `library.py` setup chronology | Home hero, recent rail, setup rail | parity |
| L-02 | — | Independent Home, Library, and Store routes with remembered route state | `umutron/catalog_ui.py` route state | Route focus/selection memory | parity |
| L-03 | — | Library pagination and focus/scroll restore | `umutron/collection.py`, `catalog_ui.py` | 60-item pages, pager, focus restore | parity |
| L-04 | P0 | Managed-library text search and genre filter | `umutron/collection.py` `library_page()` and genre choices | Pagination only; the legacy Steam-game list has search, but the managed library does not | gap |
| L-05 | — | Store search, genre filter, pagination, recently viewed | `umutron/catalog_ui.py`, `library.py` `recent_viewed` | Implemented, plus sort/order | parity |
| L-06 | P2 | Full offline Store snapshot and cached catalogue | `umutron/catalog.py` snapshots/cache | Page-level catalogue cache, not a full snapshot | partial |
| L-07 | — | Add by Steam name, App ID, or URL | `umutron/catalog_ui.py` | Implemented with select-before-add | parity |
| L-08 | P0 | Add by IGDB or manual metadata, including non-Steam entries | `umutron/catalog_ui.py`, `providers.py` | Store is IGDB-backed, but add resolves through Steam; no manual metadata-only entry | partial |
| L-09 | P1 | Metadata-provider selection, provider credentials, and manual metadata editing | `umutron/providers.py`, `metadata.py` | Automatic Steam/IGDB metadata only; no provider picker or editable metadata | gap |
| L-10 | P1 | Community/custom artwork, SteamGridDB, local PNG/JPEG | `umutron/providers.py`, `library.py` artwork | Automatic artwork only; no custom artwork path found | gap |
| L-11 | P0 | Multi-source release discovery, edition/link picker, source toggles, release sizes and compatibility labels | `umutron/sources/`, `docs/DESIGN.md` “Multi-source release selection” | Store detail only; no source links or edition discovery | gap; candidate in UmuTron docs |
| L-12 | P0 | Add an already installed game by copy or destructive move into the managed drive | `umutron/catalog_ui.py`, `game_assets.py`, `README.md` | Game Link references an external folder; no managed copy/move adoption | gap |
| L-13 | — | Link/relink an external folder and correct launchers | `umutron/detail_view.py` Game Link | Implemented | parity |
| L-14 | P1 | Unlink an external game while keeping files and the library entry | `umutron/catalog_ui.py`, `game_assets.py` | No unlink action found; remove-from-library is different | gap |
| L-15 | — | Move an installed game to another drive | `umutron/install_controller.py` | `move_managed_game` | parity |
| L-16 | — | Per-game portable backup/restore | `umutron/backup_controller.py`, `game_assets.py` | Implemented with manifest/fixes and verification | parity |
| L-17 | — | Global library ZIP backup and restore | `umutron/library.py` `export_zip`/`import_zip` | SteamTron exports per-game metadata ZIPs | not a gap; UmuTron does not expose the library ZIP in its UI |
| L-18 | P2 | Rebuild missing library entries from appmanifests | `umutron/app.py`, `game_assets.py` recovery | Partial reconciliation/repair; not the same startup recovery flow | partial |
| L-19 | P1 | Steam save migration when linking a game | `umutron/detail_view.py`, save-manager flow | Save export/import exists; no Steam-save migration on link | gap |
| L-20 | — | DLC listing, sizes, selection, download, removal | `umutron/detail_view.py`, `dlc.py`, `steam_downloader.py` | Implemented | parity |
| L-21 | P2 | DLC ownership-only lock/unlock for add-ons with no files | `umutron/detail_view.py` `Lock`/`Unlock` | Not present; SteamTron’s DLC flow is file-oriented | gap |
| L-22 | P2 | DLC size included in the library’s total game size | `umutron/detail_view.py`, `library.py` | Base size only or partial | partial |
| L-23 | P2 | DLC queue and unified download manager | `umutron/detail_view.py`, `steam_downloader.py` | Game download manager exists; DLC download has no equivalent cancel/manager UI | partial |

### Install, download, and runtime

| ID | Pri | Feature | UmuTron reference | SteamTron today | Status |
|---|---|---|---|---|---|
| I-01 | P0 | Manual Windows installer flow: select setup executable, explicit confirmation, logs, cancel/retry, installer session, prefix/runner binding | `umutron/install_controller.py`, `installations.py`, `setup_history.py` | No generic installer execution | gap |
| I-02 | P0 | Repack/archive pipeline: ZIP/RAR/7z/ISO/TAR extraction, portable versus installer classification, Inno/NSIS/MSI/InstallShield detection, silent arguments, `innoextract`, executable ranking, PE validation, staging | `umutron/payload_pipeline.py`, `download_service.py` | No equivalent; downloads depot content and detects launch targets only | gap |
| I-03 | P0 | Magnet/aria2 download path and release download staging | `umutron/download_service.py`, `umutron/sources/` | No magnet or torrent path | gap; release-source flow is a UmuTron candidate |
| I-04 | P1 | Source-reported download size, Linux/Windows icon, Proton/anti-cheat/repack labels | `umutron/sources/`, `compat.py`, `docs/DESIGN.md` | ProtonDB tier only; no release-source compatibility labels | partial |
| I-05 | — | Steam depot download through `smd` | `umutron/steam_downloader.py` | `crates/steamtron-desktop/src/download.rs` | parity |
| I-06 | P2 | One-at-a-time download queue for game and DLC jobs | `umutron/steam_downloader.py` queue | Multiple operations can coexist; no unified queue | partial |
| I-07 | — | Background download with pause, resume, cancel, progress, and resume metadata | `umutron/steam_downloader.py` | Implemented for games | parity |
| I-08 | — | Proton Manager: GE/UMU/CachyOS, downloads, verification, pause/resume/cancel, uninstall | `umutron/proton_manager.py`, `proton_ui.py` | Implemented | parity |
| I-09 | P1 | Protected runner baseline, runner leases, and process-use guard before removal | `umutron/runner_guard.py`, `proton_manager.py` | No protected baseline; `uninstall_proton_release` does not check active games or leases | gap/partial |
| I-10 | P2 | Auto-download a missing selected runner on explicit Play | `umutron/runner_preparation.py`, `launcher.py` | SteamTron errors if no runner is installed; user installs first | gap |
| I-11 | P2 | Native Linux/ELF launcher support | `umutron/launcher.py` native ELF path | Managed play requires Proton; no native ELF path | gap |
| I-12 | — | Per-game Proton, prefix source, Steam requirement, environment | `umutron/detail_view.py`, `launcher.py` | Implemented | parity |
| I-13 | P2 | Dedicated validated DLL override field | `umutron/detail_view.py` advanced launch settings | Generic environment editing only | partial |
| I-14 | — | Prelaunch script with digest approval | `umutron/prelaunch.py`, `detail_view.py` | Implemented | parity |
| I-15 | — | Fixes overlay, GoldBerg, Steamless | `umutron/fixes_overlay.py`; SteamTron `fix_tools.rs` | Implemented; additional Windows/GoldBerg/Steamless tooling | parity/ahead |
| I-16 | P2 | MangoHud, Gamescope/FSR toggles | `umutron/detail_view.py` general settings | No equivalent found | gap |
| I-17 | — | GameMode runtime toggle | `umutron/launcher.py` | Implemented | parity |
| I-18 | P3 | GameMode installation helper | `umutron/gamemode_install.py`, `settings_view.py` | Toggle only; no installer flow | partial |
| I-19 | P1 | One game or installer at a time | `umutron/operation_controller.py`, supervisor | Same-game guard only; different managed games can run concurrently | partial |
| I-20 | P1 | Supervisor survives app exit and reconnects | `umutron/launch_supervisor.py`, `operation_controller.py` | No subreaper/supervisor handoff; app process owns the child | gap |
| I-21 | P1 | Stop only verified owned processes with start-time identity | `umutron/process_evidence.py`, supervisor | Child/process-group and descendant tracking; no equivalent start-time inode verification | partial |
| I-22 | P1 | Guarded uninstall: ownership inventory, receipt, journal, interrupted recovery | `umutron/game_uninstall.py`, `docs/DESIGN.md` | `uninstall_managed_game_at` does `remove_dir_all` after basic path/process checks | gap |
| I-23 | — | Diagnostic logs, retention caps, clear | `umutron/diagnostics.py` | Implemented | parity |

### Shell and presentation

| ID | Pri | Feature | UmuTron reference | SteamTron today | Status |
|---|---|---|---|---|---|
| S-01 | — | Keyboard/controller navigation, paging, fullscreen options | `umutron/controller.py`, `operation_controller.py` | Implemented and hardened in recent navigation PRDs | parity |
| S-02 | — | Tray, close-to-tray, explicit Exit | `umutron/tray.py` | Implemented | parity |
| S-03 | P1 | Single-instance activation | `umutron/app.py` GApplication handling | No single-instance plugin found; reopening can create another process | gap |
| S-04 | P1 | Light/dark/system themes and custom theme folders | `umutron/theme_engine.py`, `settings_view.py` | Fixed dark palette in `ui/tokens.css`; no theme setting | gap |
| S-05 | P3 | Six-step UI scale | `umutron/settings_view.py`, library `ui_scale` | Implemented in fullscreen; narrower setting | partial |
| S-06 | P3 | Controller reconnect/status UI | `umutron/controller.py`, `settings_view.py` | Browser Gamepad API; no equivalent device reconnect settings | partial |
| S-07 | P3 | File-picker start-folder memory by action | `umutron/app.py` `picker_folders`, `library.py` | Not found | gap |
| S-08 | P2 | Storage volume picker, roles, capacity/reservation quotes | `umutron/storage_ui.py`, `storage_observer.py`, `storage_topology.py` | Registered drive paths and free-space checks only | partial |
| S-09 | — | Library works offline without an account | `umutron/library.py`; UmuTron saved library | SteamTron shell/actions are account-gated | product difference; decide explicitly |
| S-10 | — | Steam registration, SLSsteam, update pinning, Doctor | UmuTron has no equivalent | SteamTron-specific | ahead |
| S-11 | — | Windows Security exceptions, DDM verification, GoldBerg/Steamless | UmuTron has no equivalent or less depth | SteamTron-specific | ahead |

## Priorities and likely selection groups

These are not approvals. They are suggested bundles for the future selection
pass.

### P0 — largest product/safety decisions

- **Standalone install workflow**: `I-01`, `I-02`, `I-03`, plus `L-11` for
  release discovery. This is the largest missing subsystem. It also carries the
  highest security risk because it executes untrusted installers and extracts
  archives.
- **Library curation**: `L-04`, `L-08`, `L-12`. Search/filter, non-Steam/manual
  entries, and managed adoption of installed games.
- **Managed-library metadata**: `L-09`, `L-10`. Provider selection, manual
  metadata, custom artwork.
- **Release-source UI**: `L-11` and `I-04` only after confirming UmuTron’s
  candidate branch actually ships the behavior.

### P1 — important parity and lifecycle

- **Safety/lifecycle**: `I-19`, `I-20`, `I-21`, `I-22`, `I-09`.
- **Library workflows**: `L-14` unlink, `L-19` Steam save migration.
- **Presentation**: `S-03` single instance, `S-04` themes.

### P2 — meaningful but narrower

- `L-06` full offline snapshot, `L-18` manifest recovery, `L-21` DLC
  ownership, `L-22` DLC-inclusive size, `L-23` unified DLC manager.
- `I-06` download queue, `I-10` runner auto-download, `I-11` native ELF,
  `I-13` DLL override field, `I-16` MangoHud/Gamescope.
- `S-08` storage topology/reservations.

### P3 — polish

- `I-18` GameMode install helper, `S-05` wider UI scale, `S-06` controller
  reconnect, `S-07` picker memory.

## Requirements

- R1: No item from this PRD is implemented until the team explicitly selects
  its ID and records the selection in a follow-up PRD or a dated decision in
  this file.
- R2: Every selected item must preserve SteamTron’s existing Steam-specific
  behavior: registration, SLSsteam integration, update pinning, Doctor,
  GoldBerg/Steamless, Windows Security exceptions, and DDM verification.
- R3: Every selected item must preserve the current navigation/focus contract,
  including independent route focus and the Home/Library/Store focus fixes.
- R4: Every selected item must preserve existing library data, backups, DLC
  state, and prefix compatibility. Any migration must be reversible or
  explicitly documented as one-way.
- R5: Features marked as UmuTron candidates must be verified in the actual
  UmuTron branch before selection. Documentation alone is not sufficient.
- R6: No selected item may port code-only or removed UmuTron behavior listed in
  “What not to port”.
- R7: Any installer, archive, magnet, or source-link implementation must use
  explicit user confirmation and must not execute unreviewed payloads
  automatically.
- R8: Any deletion/uninstall feature must define ownership evidence, refusal
  conditions, interruption behavior, and recovery before implementation.
- R9: Any theme or visual item must remain readable in desktop and fullscreen
  modes, preserve visible focus, and avoid hard-coded colors outside the
  token layer.
- R10: Every selected item must update the affected PRD, verification evidence,
  and index in the same commit as its implementation.
- R11: The product decision for account-gated versus offline library access
  (`S-09`) must be recorded before implementing related catalog or library
  features.
- R12: Selection must explicitly state whether SteamTron is targeting
  “Steam-integrated manager” behavior, “standalone launcher” behavior, or both.
  The installer/source/native-launch items depend on this decision.

## Acceptance criteria

- [ ] The selection pass names the chosen backlog IDs and the rejected IDs.
- [ ] For each chosen item, observable behavior matches the corresponding
  UmuTron reference behavior or a documented SteamTron-specific adaptation.
- [ ] SteamTron’s existing Steam registration, update, Doctor, fixes, security,
  and verification behavior is unchanged.
- [ ] Existing Home/Library/Store navigation and focus behavior is unchanged.
- [ ] Existing library records and backups still load; migrations have rollback
  or explicit compatibility notes.
- [ ] Installer/source/archive items require explicit confirmation and never
  execute an unreviewed payload.
- [ ] Uninstall items define ownership checks, refusal cases, interruption
  recovery, and evidence in the implementation PRD.
- [ ] Theme items work in desktop and fullscreen and retain visible focus.
- [ ] Single-instance behavior activates the existing window instead of starting
  a second app instance.
- [ ] All verification evidence is recorded in the affected PRD and the index
  status is updated.

## Verification

No behavior was implemented or executed for this PRD. This is a research-only
artifact.

Evidence used for the audit:

- Source and documentation inspection of UmuTron at `4b60545`, including
  `README.md`, `docs/DESIGN.md`, `docs/STORAGE.md`,
  `docs/AUTOMATED-INSTALLATION-PIPELINE.md`,
  `docs/DOWNLOAD-SERVICE-CONTRACT.md`, and the modules cited in the tables.
- Source and documentation inspection of SteamTron at `f1f0d77`, including
  `crates/steamtron-desktop/src/main.rs`,
  `crates/steamtron-desktop/src/download.rs`,
  `crates/steamtron-desktop/src/proton_sources.rs`,
  `crates/steamtron-desktop/ui/app.js`,
  `crates/steamtron-desktop/ui/tokens.css`, and
  `docs/UMUTRON-BEHAVIOR-REFERENCE.md`.
- No tests, builds, binaries, or runtime checks were run for this PRD.

Future selected items must add their own verification commands and observed
results. A selection-only update does not satisfy that requirement.

## Decisions and risks

### Decisions recorded here

1. This document is a backlog, not a mandate for full parity.
2. SteamTron’s Steam-specific features are treated as advantages, not gaps.
3. The team will select IDs explicitly rather than implementing the table
   wholesale.
4. Candidate UmuTron behavior must be verified before it becomes a requirement.
5. Dead or unexposed UmuTron code will not be ported merely because it exists.

### Risks

- **Installer and archive execution is the highest-risk area.** UmuTron’s
  payload pipeline executes or extracts untrusted content. SteamTron must not
  copy that behavior without a confirmation, ownership, path-traversal,
  symlink, and failure-recovery design.
- **Release-source discovery is a candidate feature.** UmuTron’s
  `docs/DESIGN.md` labels multi-source release selection as a feature-branch
  candidate. Its public endpoint currently returns magnets only, and the
  desktop cannot display withheld links. Confirm the shipped state before
  selecting it.
- **Installer wording conflicts across UmuTron docs.** Its README says some
  automatic download/staging/installer workflows are not exposed, while other
  docs and tests describe explicit installer and payload flows. Distinguish
  explicit user-driven installation from fully automatic execution.
- **Uninstall safety is materially weaker in SteamTron today.** The current
  uninstall path performs a recursive deletion after basic checks. Any
  selection involving uninstall should prioritize UmuTron-style ownership and
  recovery evidence.
- **Supervision is a product architecture change.** A detached supervisor and
  reconnect flow affects process ownership, logs, tray exit, and crash
  recovery. It is not a UI-only change.
- **Native Linux launch may conflict with SteamTron’s registration model.**
  Decide whether native games are registered, launched outside Steam, or
  excluded from managed registration before implementation.
- **Single-instance behavior interacts with tray exit.** Closing to tray must
  not make a second launch appear to do nothing. Activation must raise the
  existing window.
- **Themes touch every surface.** Fullscreen, modals, disabled controls, focus
  rings, and system-theme changes need coverage. A theme engine that only
  recolors the main page is not parity.
- **Storage topology in UmuTron is partly a boundary contract.** Its own
  documentation says automatic install/download is not fully connected and
  the registry is metadata-only. Do not port it as an active transfer feature.
- **Offline/account behavior is unresolved.** UmuTron’s saved library works
  offline; SteamTron’s shell is account-gated. This is a product decision, not
  an automatic parity item.
- **Global library ZIP is dead code in UmuTron.** `library.export_zip` and
  `import_zip` exist, but no UI wiring was found. SteamTron’s per-game metadata
  ZIPs are the working behavior; do not add a global ZIP solely for symmetry.
- **Prefix inventory/runtime maintenance is removed or historical.** It should
  not be selected from the old audit without rechecking UmuTron’s current UI.
- **Activity drawer is intentionally absent.** Do not reintroduce it as part of
  parity.

## What not to port

- Global library ZIP creation/import as a user-facing feature. UmuTron’s
  methods are not exposed in its current UI.
- UmuTron’s per-game Prefix tab and global prefix management contract. Current
  UmuTron documentation says the per-game tab was removed and the global work
  is not part of the current UI.
- UmuTron’s storage automatic-install execution. Its own docs describe the
  boundary as metadata-only and not connected to transfer execution.
- The Activity drawer and per-game Activity tab. SteamTron deliberately removed
  them; logs and Doctor carry the relevant information.
- UmuTron’s GTK widget structure, GApplication-specific code, and exact visual
  styling. Only behavior and safety contracts apply.
- UmuTron’s standalone/no-Steam account model as an automatic replacement for
  SteamTron’s account and registration model. This requires an explicit product
  decision (`S-09`, `R12`).

## Already at parity or ahead

These are recorded so later selection does not treat them as missing:

- Home hero, recent-play rail, and ready-to-play rail.
- Separate Home/Library/Store routes with independent focus and route state.
- Library pagination and focus/scroll restore.
- Store search, genre filter, pagination, sort/order, and recently viewed.
- Add by Steam name/App ID/URL with select-before-add.
- Link/relink external folders and correct launchers.
- Move installed games between drives.
- Per-game portable backup/restore and free-space checks.
- DLC listing, sizing, selection, download, and removal core.
- Steam depot download and background download controls.
- Proton Manager sources, download, verification, pause/resume/cancel, uninstall.
- Per-game Proton, prefix source, Steam requirement, environment, prelaunch.
- Fixes overlay, GoldBerg, Steamless, update pinning, Doctor, DDM verification,
  Windows Security exceptions.
- Keyboard/controller navigation, paging, fullscreen options, tray behavior.
- Diagnostic logs and retention.
- Steam registration and SLSsteam integration.

## Links

- UmuTron README: `/mnt/krunal/WorkSpace/Code/UMUTron-WorkSpace/UmuTron/README.md`
- UmuTron design: `docs/DESIGN.md`
- UmuTron storage contract: `docs/STORAGE.md`
- UmuTron installer pipeline: `docs/AUTOMATED-INSTALLATION-PIPELINE.md`
- UmuTron download contract: `docs/DOWNLOAD-SERVICE-CONTRACT.md`
- UmuTron source adapters: `umutron/sources/README.md`
- SteamTron behavior reference: `docs/UMUTRON-BEHAVIOR-REFERENCE.md`
- Related focus PRDs: `0002-nav-focus.md`, `0005-route-focus.md`,
  `0007-detail-focus.md`
- Related DLC/cache PRDs: `0055-dlc-queue-and-install.md`,
  `0056-dlc-sizes.md`, `0057-dlc-ddm-direct.md`,
  `0058-dlc-toggle-without-reload.md`,
  `0059-dlc-download-manager-state.md`,
  `0062-dlc-content-group-progress.md`,
  `0063-dlc-downloading-button.md`,
  `0064-dlc-primary-button-progress.md`,
  `0065-cache-dlc-catalogue.md`, `0066-cache-updates-tab.md`,
  `0067-per-game-cache-sidecar.md`
- SteamTron revision audited: `f1f0d77`
- UmuTron revision audited: `4b60545`
