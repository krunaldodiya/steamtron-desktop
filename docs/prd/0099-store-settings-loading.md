# Keep Settings and Store shells during loading

- Status: done
- Date: 2026-11-18
- Areas: `crates/steamtron-desktop/ui/app.js`

## Problem

Two loading states replaced more UI than intended on Windows.

1. Store search: typing a query and pressing Enter removed the Store header's
   body — sidebar, filters, and results — and left only "Discover your next
   game" plus "Loading the Store…". The search should blank only the results
   area.
2. Settings → Storage: opening the tab showed "Loading…" in the whole settings
   body and stayed there. The backend is fast: measured `available_library_drives`
   at 211 ms and `renderStorageSettings` at 7 ms through the running webview, so
   the tab had no bound on an awaited IPC call.

Store cause: `resetStorePaging()` empties `storePages` on a new query, then
`ensureStorePage()` paints. `renderStorePage()` had an early return
`if (!storeConnection && !storeLoadedItems().length)` that rendered a full-page
"Loading the Store…" template. A cached catalogue can render while
`storeConnection` is still `null`, so the next search (cache now empty) fell
into that branch.

Reproduced on the installed build through the webview CDP endpoint: after the
search, `#store-search` and `.store-content` were both absent and the body
contained only the loading template.

## Goal

The Store keeps its header, sidebar, and layout while a search or page load
runs, and Settings tabs always paint content (or an error) instead of waiting
forever on one IPC call.

## Non-goals

- Changing store pagination, IGDB requests, or drive enumeration.
- Adding a generic timeout to every `invoke` in the UI.
- Redesigning the Store or Settings layouts.

## Requirements

- R1: `renderStorePage()` never replaces the whole body with "Loading the
  Store…"; the connect/loading state renders inside the normal Store layout.
- R2: A search clears only the results region; the sidebar and search box stay
  mounted so focus and typed text survive the repaint.
- R3: Storage settings bound both awaited IPC calls (library snapshot and drive
  enumeration) and render with fallbacks when one stalls.
- R4: Storage still shows an error via the existing catch path when rendering
  itself fails.

## Acceptance criteria

- [x] After the fix, searching keeps the shell and shows the loading state only
  inside the results grid.
- [x] Settings → Storage renders in milliseconds when the backend responds.
- [x] Settings → Storage renders after at most 4 s even if an IPC call never
  settles.

## Verification

- CDP reproduction on build `237a0590…`: `{sidebar:false, content:false,
  body:"…Loading the Store…"}` after entering `Little` and pressing Enter.
- Direct calls in the webview: `available_library_drives` returned 3 drives in
  211 ms; `renderSettingsPage()` with `settingsTab="storage"` completed in 7 ms.
- Post-fix build, same CDP scenario: after the search the sidebar and
  `.store-content` stayed mounted and `#store-results` listed the matching
  titles; the page header remained "Discover your next game".
- Post-fix Storage: `settings-loading` was absent and the drive list rendered.
  `settleWithin(new Promise(() => {}), 300, "fallback")` returned `fallback` in
  301 ms.
- Installed build `4e836549274c5ed984e5a04d40c2365bbebd77546a55a19bc4cb36f164f07119`
  at 18:33 matches `target/release/steamtron-desktop.exe`; the app was restarted
  from the installed path.

## Decisions and risks

- The Store shell is always rendered now; a first open with unknown IGDB status
  shows the normal layout with a loading status instead of a dedicated splash.
- `settleWithin` is intentionally local to settings. A stalled IPC call can
  still leave a stale tab repaint if it resolves after the user switches tabs;
  the 4 s fallback bounds how long the wrong state is visible.

## Links

- Related: `0084-store-warmup-flash`, `0097-windows-launch-log`.
