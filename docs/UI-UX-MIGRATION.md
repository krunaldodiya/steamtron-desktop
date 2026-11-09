# SteamTron Desktop — UI/UX migration from UmuTron

Status: proposal
Companion to `UMUTRON-TO-STEAMTRON-MIGRATION-PRD.md`
Date: 2026-11-09

UI/UX is a primary deliverable, not polish at the end. SteamTron keeps the
Tauri shell and its cross-platform features, but adopts UmuTron's information
architecture, visual language, and couch interactions. Every retained SteamTron
feature must fit an existing UmuTron surface instead of inventing a parallel one.

## 1. Principles (adopted from UmuTron)

1. **Content first.** Covers and heroes carry the screen; chrome stays quiet.
2. **One primary action per screen.** Play/Stop is the only filled action on the
   detail page; destructive actions are separate and confirmed.
3. **Status has one home.** Runtime/prefix on Compatibility, overlay on Fixes,
   tools in their sections, registration on the add flow, logs in Debug,
   backend validation in Doctor. No duplicated activity feed.
4. **Explain in place.** A disabled control states why in one sentence, with the
   exact path or check that fails.
5. **Couch-ready.** Focus is always visible; every action is reachable with a
   controller; layouts scale for a TV (100–200 % UI scale).
6. **Never hide failures.** Toasts confirm; inline text and Debug carry detail.
7. **No dead ends.** Errors name the next action (repair, choose folder, start
   Steam, re-register).

## 2. Screen map

| Screen | Role | UmuTron source | SteamTron features folded in |
|---|---|---|---|
| Shell/header | Brand, Home/Library/Store, search, add, settings, account, mode | Window header | Add Game, restart banner, account menu |
| Home | Hero, Recently played, Ready to play | Home view | Play/Stop entry points |
| Library | Search, cover grid, pagination, empty state | Library grid | Dual-registered entries, readiness |
| Detail | Hero, cover, meta, description, Play/Stop, manage | Detail page | Steam App ID, registration state, backup |
| Game Settings · General | Name, launch targets, runtime, sizes | Detail tabs | Launch target editor, security (Windows) |
| Game Settings · Compatibility | Runtime, prefix source, launch mode, Steam requirement, resolved paths | Compatibility page | Windows security, VDF launch options (Steam mode) |
| Game Settings · Fixes | Import/export, location, overlay, GoldBerg, Steamless | Fixes page | Existing ZIP engine and overlay backends |
| Game Settings · Environment | Key/value variables + overrides derived from fixes | Environment page | Windows/Linux per-game env |
| Game Settings · Game Link | Managed/linked folder, launchers | Game Link page | Steam library link, import |
| Game Settings · Debug | Run log, evidence, controller state | Debug page | Doctor links for backend failures |
| Game Settings · Installed Files | Paths, sizes, integrity | Installed Files | Backup/restore entry |
| Game Settings · Updates / DLC | Optional Steam-mode surfaces | Updates/DLC tabs | Manifest pinning (optional) |
| Add Game | Stepper: source → details → dual registration → done | Setup/Link dialog | Lua/manifest, folder link, provider import |
| Doctor | Backend validation (SLSsteam-moon / BetterSteamTools) | none | SteamTron doctor, scoped |
| Settings · General | Display mode, gamemode, controller | Global General | Controller reconnect |
| Settings · Storage | Drives, library layout, backups | Global Storage | Drive registration, restore |
| Settings · Runtime | Proton/UMU runners | Proton Manager | Runner downloads, defaults |
| Settings · Diagnostics | Logs, bundle export | Diagnostics | Debug bundle |
| Settings · Appearance | Theme, scale | Appearance | Webview tokens |

Removed: global Activity drawer, per-game Activity tab, Activity count badge.
Debug + Doctor replace them.

## 3. Feature mapping (SteamTron → UmuTron surface)

| SteamTron feature | Where it lives now | Change |
|---|---|---|
| Add game (Steam entry only) | Add Game stepper | Same flow also creates the managed record; two status rows (Library, Steam) |
| Lua/manifest import | Add Game → source step | Optional source; never required for Steam-free play |
| Unlock backend selection | Add Game → Steam row + Doctor | Automatic by platform; Doctor validates and repairs |
| Launch options (`localconfig.vdf`) | Compatibility → Steam mode (advanced) | Hidden unless launch mode uses the Steam client |
| Multiple launch targets | General + detail launcher picker | Same as UmuTron; stale targets pruned |
| Fixes ZIP / overlay | Fixes tab | Same sections and order as UmuTron |
| GoldBerg / Steamless | Fixes tab (bottom) | Apply/Remove, same wording and states |
| Backups/restore | Storage + detail backup action | Restore into the selected drive |
| Doctor | Top-level screen / Settings | Backend-only; no game checks |
| Updates/pinning | Updates tab (optional) | Only for Steam-mode games |
| Activity (global + per-game) | — | Removed |
| Auth/profile | Header account menu | Unchanged |
| Restart-Steam banner | Shell banner | Only when a registration requires a client restart |

## 4. Components and rules

| Component | Spec |
|---|---|
| Card | Cover-first, 2:3 art, title below, optional readiness dot; collection width drives columns; focus ring on the whole card |
| Hero | Full-bleed art, gradient scrim, bottom-left text block; no carousel controls |
| Section | Heading + one-line description + content; `Rule` separator between sections |
| Row | Title, description, right-aligned control; disabled rows keep the reason visible |
| Tabs | Stacked pages in Game Settings; keyboard/controller left/right changes tabs |
| Status chip | Small capsule: ready, needs setup, registered, pending restart, error |
| Primary action | Filled accent; only Play/Stop on detail, only Confirm in destructive dialogs |
| Progress strip | Determinate bytes when known, otherwise stage text; always cancellable |
| Confirm dialog | Title as a question, consequence sentence, destructive button labeled with the action |
| Path display | Selectable monospace text; never truncated without a tooltip |
| Picker | Remembers the last folder per action (fixes import/export, backup source/destination, savers, installers) |
| Toast | Short confirmation only; failures also appear inline |
| Empty state | States what is missing and the next action |

## 5. Design tokens

```css
:root {
  --surface:  #0f1116;  --surface-raised: #171a21;  --border: #262b36;
  --text: #e8ecf3;      --muted: #93a0b4;
  --accent: #7fd8c8;    --accent-soft: rgba(127,216,200,.14);
  --success: #79d18b;   --warning: #e2b75a;  --error: #e27676;
  --radius: 12px;       --radius-lg: 18px;
  --space-1: 4px; --space-2: 8px; --space-3: 12px; --space-4: 16px; --space-6: 24px;
  --font-ui: system-ui, "Inter", "Segoe UI", sans-serif;
  --scale: 1; /* 1.00 / 1.25 / 1.50 / 1.75 / 2.00 for TV */
}
```

- Dark is default; light uses the same roles with inverted surfaces.
- Accent is reserved for focus, primary action, and selection.
- Type scale: 28/22/17/14/12 px at 100 % scale, multiplied by `--scale`.
- Motion: 120–180 ms ease-out; honor `prefers-reduced-motion`.
- No background images behind text except the hero scrim.

## 6. Interaction model

| Input | Action |
|---|---|
| Controller A | Select / open |
| Controller B | Back / close dialog / leave detail |
| Controller X | Play/Stop on detail |
| Controller Y | Open game menu (manage) |
| LB / RB | Previous / next page in collections |
| D-pad / left stick | Move focus; repeats after 400 ms, then every 160 ms |
| Keyboard | Arrows move focus, Enter selects, Esc backs out |
| Mouse | Everything reachable; wheel scrolls collections |

- Focus order follows visual order; dialogs trap focus and restore it on close.
- Focused items scroll into view; focus ring is always visible.
- Input is accepted only while a launcher-owned window is active; a stale SDL
  device reconnects on focus (UmuTron behavior).
- Destructive actions never sit next to the primary action.

## 7. Data contract (Tauri)

Commands per surface (names indicative; Rust returns typed errors):

| Surface | Commands |
|---|---|
| Library | `list_games`, `import_stray`, `remove_game` |
| Add flow | `add_preview`, `add_commit`, `registration_status`, `registration_retry` |
| Detail | `game_detail`, `play`, `stop`, `run_state`, `recent_play` |
| Compatibility | `launch_settings`, `save_launch_settings`, `resolve_paths` |
| Fixes | `fixes_info`, `fixes_import`, `fixes_export`, `fixes_toggle`, `goldberg_apply/remove`, `steamless_apply/remove` |
| Environment | `environment_get/set` |
| Debug | `run_logs`, `diagnostics_bundle`, `controller_status` |
| Doctor | `doctor_report`, `doctor_repair` |
| Storage | `drives_list`, `drive_register`, `backup_export/import` |
| Runtime | `runners_list`, `runner_select`, `runner_install` |

Events: `run_state`, `run_log`, `progress`, `registration_changed`,
`controller_state`. The UI never polls for run state.

## 8. Accessibility

- Every control keyboard reachable; ARIA labels on icon buttons.
- Contrast ≥ 4.5:1 for text, ≥ 3:1 for focus rings.
- Reduced motion disables transitions.
- Screen-reader names for cards include the readiness state.
- Toast and banner use `aria-live="polite"`.

## 9. UI-first delivery slices

The UI is built alongside the backend, not after it. Each slice ships a runnable
screen using real commands with fixtures where the backend is pending.

| Slice | Ships | Acceptance |
|---|---|---|
| U0 | Design system + shell (tokens, header, routes, toast/modal layers) | Visual parity with UmuTron shell at 100 % and 150 % |
| U1 | Library + Home with real data and empty states | Grid, search, pagination, readiness, keyboard focus |
| U2 | Detail + Play/Stop + Compatibility | Launch state, resolved paths, launch mode, requirement |
| U3 | Add Game stepper with dual registration | Library/Steam rows and all states; repair in place |
| U4 | Fixes (import/export, location, overlay, GoldBerg, Steamless) | Same sections/order/wording as UmuTron |
| U5 | Doctor + Settings (General, Storage, Runtime, Diagnostics) | Backend validation and repairs only |
| U6 | Game Link, Installed Files, Environment, Debug | Full tab parity |
| U7 | TV/fullscreen + controller | 1920×1080, 1280×720, 1024×600; controller-only walkthrough |

## 10. Visual and interaction testing

- Golden screenshots per slice at 1920×1080, 1280×720, 1024×600, dark and light,
  UI scale 100/150.
- Controller walkthrough test: library → detail → play → back → settings, no mouse.
- Path/status fixtures for every registration state and every launch mode.
- Snapshot diffs reviewed as part of the PR; layout regressions block merge.

## 11. Risks

| Risk | Mitigation |
|---|---|
| Webview layout differs from GTK | Token-driven CSS, golden screenshots on both OSes |
| Large libraries (1000+ covers) | Virtualized grid or paged rendering; lazy artwork |
| Controller in webview | Rust `gilrs` loop in the backend; UI receives actions as events |
| Platform font/GPU differences | Ship fallback fonts; test Cairo/WebKitGTK and WebView2 |
| Two backends of “status” creeping back | Enforce the “status has one home” table in review |
