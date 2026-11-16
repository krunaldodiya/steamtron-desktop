# Direct IGDB Store with per-user backend token

- Status: done
- Date: 2026-11-16
- Areas: `UmuTron-FastAPI` (`app/igdb_tokens.py`, `app/routers/igdb.py`, `app/ui.py`, `app/db.py`, `app/main.py`), `crates/steamtron-desktop` (`src/igdb.rs`, `src/main.rs`, `ui/app.js`), `crates/steamtron-auth`

## Problem

The SteamTron Store currently reaches IGDB through two hops:

```text
SteamTron desktop -> UmuTron FastAPI /v1/games -> Twitch/IGDB
```

Every Store page, genre list, and detail request therefore pays for the
FastAPI route, the backend IGDB client, and a second upstream request. The
backend also owns the only token cache, so the desktop cannot make direct IGDB
queries and cannot show a precise connection state when IGDB is unavailable.

The requested behavior is:

- Use a signed-in user’s IGDB credential from the backend.
- If the credential is already stored, return it without regenerating.
- Cache it locally for the desktop session.
- Query IGDB directly from the Store.
- If no credential exists, leave the Store empty and offer a connection flow
  through the FastAPI settings page.
- Handle expired and invalid credentials without making the Store unusable.

### Credential-model finding

IGDB authenticates with a **Twitch application access token** obtained through
the client-credentials grant. It is not a per-user Twitch OAuth token. A
Twitch user-login flow does not by itself grant IGDB access. The implementation
therefore uses the requested per-user database/connect UX, but the credential
stored is the Twitch application token derived from the server’s
`TWITCH_CLIENT_ID` and `TWITCH_CLIENT_SECRET`. The settings page must explain
that it connects the IGDB application, not a personal Twitch account.

## Goal

- The Store calls IGDB directly after one token lookup per app session.
- The backend is involved only for token status, initial connect, and refresh;
  it is not in the path of every catalog request.
- Missing, expired, revoked, and invalid tokens have explicit, recoverable UI.
- No IGDB credential is exposed in logs, backups, URLs, or browser storage.

## Non-goals

- Removing the existing FastAPI catalog endpoints. They remain for
  compatibility and other clients.
- Adding a personal Twitch OAuth identity, Twitch follows, or Twitch profile
  features. The IGDB credential is an application token.
- Persisting the IGDB access token in `localStorage`, a URL, or a portable
  backup.
- Changing Store navigation, focus, pagination, Steam resolution, ProtonDB, or
  Add-to-library behavior.
- Adding background IGDB sync, recommendations, or catalog mirroring.
- Deploying automatically from this repository. Deployment is an operator step
  after the backend change is pushed.

## Design

### Backend storage

Add a Turso/SQLite-compatible table:

```sql
CREATE TABLE IF NOT EXISTS igdb_tokens (
  user_id TEXT PRIMARY KEY,
  client_id TEXT NOT NULL,
  access_token TEXT NOT NULL,
  expires_at INTEGER NOT NULL,
  updated_at TEXT NOT NULL
);
```

The row is per signed-in user. The access token is the Twitch application
token; per-user rows make the requested connection state explicit without
putting credentials in client-readable catalog responses.

### Backend endpoints

- `GET /v1/igdb/status`
  - Authenticated.
  - Returns `configured`, `connected`, `expires_at`, and `connect_url`.
  - Never returns the access token.
  - If a stored token is expired or within the refresh window, attempt a
    refresh before reporting the status.
- `GET /v1/igdb/token`
  - Authenticated.
  - Returns `client_id`, `access_token`, `expires_at`, and the fixed IGDB base
    URL.
  - Refreshes an expired/expiring token.
  - Returns `IGDB_NOT_CONNECTED` when no row exists.
  - Returns `IGDB_NOT_CONFIGURED` when the server lacks Twitch credentials.
  - Sends `Cache-Control: no-store`.
- `POST /v1/igdb/connect`
  - Authenticated.
  - Generates or refreshes the application token for the current user.
  - Used by API clients; the browser settings page may call the service
    directly.

### Backend settings page

Add `/ui/settings/igdb`:

- Shows server configuration state.
- Shows connected/disconnected and token expiry.
- Provides **Connect IGDB** and **Refresh token** actions.
- Explains that IGDB uses a Twitch application token.
- Returns to the page with a success/error result after the POST.
- The existing header gains a **Settings** link for signed-in users.

### Desktop token handling

- Add `igdb::status` and a session-level token cache in
  `crates/steamtron-desktop/src/igdb.rs`.
- The access token is cached in process memory only. It is never written to
  `localStorage`, a URL, a log, or a backup.
- Token refresh rules:
  - Desktop refreshes from the backend when the cached token is missing or
    within 120 seconds of expiry.
  - Backend refreshes when the stored token is missing, expired, or within
    300 seconds of expiry.
  - An IGDB `401` invalidates the desktop cache, fetches a fresh backend token,
    and retries the request once.
  - A second `401` returns a reconnect-required error instead of looping.
- Serialize direct IGDB requests through a small local lane with a 260 ms
  minimum spacing, matching the previous backend lane.
- Direct IGDB calls use the fixed host `https://api.igdb.com/v4`, the
  `Client-ID` header, and the bearer token. No user-controlled URL is accepted.

### Desktop Store flow

1. On Store entry, call `igdb_store_status`.
2. If not configured: show an empty Store state explaining the server has no
   Twitch credentials. Offer Settings only if a settings URL exists.
3. If configured but not connected: show an empty Store state with
   **Connect IGDB**. The button opens the backend settings page in the browser.
4. While the browser connection is pending, poll status at a bounded interval.
   When connected, reset Store paging and load the first page.
5. If connected: load genres and pages directly from IGDB.
6. If a direct request later fails with `IGDB_NOT_CONNECTED` or repeated
   `401`, show the same connection state.
7. Cached pages and genres remain usable when IGDB is temporarily unreachable.

### Expiry and failure behavior

| State | Backend | Desktop |
|---|---|---|
| No token row | `status.connected=false`, `IGDB_NOT_CONNECTED` on token | Store empty with Connect |
| Token within refresh window | Refresh and return new token | Accept new token |
| Token expired, refresh succeeds | Replace row and return new token | Continue |
| Token expired, refresh fails | Report not connected/unavailable | Empty/connect state, cached catalog remains |
| Twitch credentials absent | `IGDB_NOT_CONFIGURED` | Empty state; do not retry in a loop |
| IGDB returns `401` | Not involved | Clear cache, fetch token, retry once |
| IGDB returns `429` | Not involved | Rate-limit message with retry |
| Backend unreachable | Not involved | Keep cached pages, show retry |

## Requirements

- R1: Add the `igdb_tokens` schema and initialize it on both local SQLite and
  Turso.
- R2: Add authenticated status/token/connect endpoints with no token leakage
  in status responses.
- R3: Store one token row per user and refresh it on expiry.
- R4: Add the backend settings page and connect/refresh actions.
- R5: Add a desktop IGDB module with in-memory token caching and expiry
  handling.
- R6: Replace Store genres, browse, and detail calls with direct IGDB queries
  normalized to the existing Store shape.
- R7: Keep existing page/genre cache behavior and cache the new direct
  responses.
- R8: Show an explicit empty/connect state when IGDB is not connected or not
  configured.
- R9: Open the backend settings page from the Store and poll for completion.
- R10: Retry a direct IGDB request once after a `401`; never loop indefinitely.
- R11: Preserve existing Store pagination when `total_pages` is unavailable by
  honoring `has_next`.
- R12: Keep the legacy backend catalog endpoints working.
- R13: Never place the IGDB token in logs, URLs, backups, or browser storage.
- R14: Add backend tests for connect, status, token retrieval, expiry refresh,
  unknown-user isolation, and missing credentials.
- R15: Add desktop unit tests for query construction/normalization and token
  freshness decisions where practical without live IGDB calls.
- R16: When IGDB is not connected, not configured, or the status probe fails,
  the Store must render only the connection/error card. Hide the search
  sidebar, sort/genre lists, recently viewed chips, results grid, pager, and
  page hint until IGDB is connected.
- R17: Opening a warm Store must paint the cached genre list and cached
  catalogue page before any token/status network probe. The default session
  must not show a blocking “Checking IGDB connection…” screen when cached
  content exists.
- R18: Once the IGDB token is cached in the Rust process, `igdb_store_status`
  must answer from that cache without calling the FastAPI status endpoint.
  The backend status call is a cold-session/connection-recovery operation, not
  a per-Store-open operation.
- R19: Store pages contain 36 items. The IGDB request uses `limit 37` and the
  UI page contract uses 36 so the extra row can determine `has_next`.
- R20: Search text, genre, and PC platform filtering are applied in the IGDB
  query before the 36-item page is cut. The UI must not fetch 36 items and then
  filter them client-side by genre, search, platform, or Steam availability.
- R21: The Store uses independent page entries keyed by page number. A page
  that is not cached is fetched directly by its page number; loading page 4
  must not depend on pages 1–3 or skip a page. Existing 24-item catalogue cache
  entries are invalidated by a versioned cache key.

## Acceptance criteria

- [ ] A signed-in user with no token sees an empty Store and a working
  **Connect IGDB** action.
- [ ] Connecting through the backend page stores a row and the Store loads
  without a per-page FastAPI catalog call.
- [ ] The desktop obtains the token once per session and reuses it until the
  refresh window.
- [ ] Expired tokens are refreshed automatically by the backend.
- [ ] A revoked/invalid token triggers one refresh/retry and then a clear
  reconnect state.
- [ ] Missing Twitch credentials produce a distinct non-looping state.
- [ ] Disconnected, unconfigured, and status-error states show only the
  connection/error card; no sidebar, recent chips, results, or pager.
- [ ] Cached Store pages remain usable when IGDB is offline after reconnecting;
  the disconnected screen itself is connection-only.
- [ ] A warm Store open paints cached genres and catalogue content without a
  blocking connection check.
- [ ] Repeated Store opens in one app session do not call the FastAPI status
  endpoint after the token is cached.
- [ ] Cached genres are reused from `store_cached_genres`; a network genre
  fetch occurs only when the local cache is absent or empty.
- [ ] Every non-final Store page renders up to 36 items, including page 4 and
  page 5 in the reported case.
- [ ] Genre, search, and PC platform constraints are present in the IGDB
  request; the UI does not post-filter the returned page.
- [ ] Page 4 fetches page 4 directly when uncached and does not skip page 5 or
  depend on pages 1–3.
- [ ] Old 24-item cache entries are not reused as 36-item pages.
- [ ] Direct IGDB browse, genre, detail, pagination, Steam resolution, and Add
  to library behavior remain intact.
- [ ] No token appears in logs, URLs, backups, `localStorage`, or command
  output.
- [ ] Existing FastAPI catalog tests and new token tests pass.
- [ ] `cargo check` and the narrow desktop tests pass.

## Verification

Recorded after implementation:

- `uv run --extra dev pytest tests/test_igdb_tokens.py -q`: 7 passed.
  Covers missing configuration, connect, token reuse, expired-token refresh,
  per-user isolation, missing-credential rejection, and the authenticated
  settings page.
- `uv run --extra dev pytest -q`: full backend suite passed.
- `uv run --extra dev ruff check app/igdb_tokens.py app/routers/igdb.py
  tests/test_igdb_tokens.py`: passed.
- `cargo check -p steamtron-desktop`: passed.
- `cargo test -p steamtron-desktop`: 26 passed.
- `node --check crates/steamtron-desktop/ui/app.js`: passed.
- `bash scripts/build-linux.sh`: deb and AppImage built; installed binary
  SHA-256 matched the release build
  (`cd4703982c1d607402424f4b1a3e502e0198ceffdc8b26c9ce309b628295efaf`).
- Local UI verification against an isolated FastAPI instance and isolated app
  data/cache:
  - With no cached catalogue and no connection, the Store showed only the
    connection card. The sidebar, recently viewed chips, pager, page hint, and
    results grid were absent.
  - With a warm local catalogue, opening Store painted the cached genres,
    recently viewed chips, and catalogue cards immediately instead of showing
    the blocking “Checking IGDB connection…” card. The release build was
    captured 0.3 seconds after the Store click.
  - Repeated Store opens reuse the in-process token/status state; the FastAPI
    status endpoint is not called after a valid token is cached.
- Genre cache path verified: `store_cached_genres` is read before the status
  probe, and the screen rendered the cached genre sidebar with the cached
  page.
- 36-item pagination verified against live IGDB:
  - Direct page 1, page 4, and page 5 requests each returned 37 rows
    (`limit 37`; the UI uses the first 36 and `has_next`).
  - The UI rendered 36 cards on page 4 and 36 cards on page 5.
  - The pager showed page 4 as active with 36 titles per page; no page was
    empty or reduced to a single card.
- Server-side filtering verified:
  - `Shooter` genre plus PC platforms returned 37 rows with every row
    containing that genre.
  - A text search plus PC platforms returned 37 rows for the first page.
  - The UI no longer filters the returned page by platform or Steam
    availability; those filters were removed from the page path.
- Page isolation verified: pages 1, 4, and 5 loaded independently by page
  number; the page map count reached 108 loaded titles without requiring a
  contiguous 1–5 prefetch.
- Cache versioning: catalogue keys now include `v2|36`, so old 24-item cache
  entries are ignored rather than rendered as short pages.
- Production deployment verified after pushing backend `224efbf`:
  - `GET /v1/igdb/status` returned 200 with `configured=true` and
    `connected=false` before connection.
  - `POST /v1/igdb/connect` returned 200 without printing the token.
  - A direct `POST https://api.igdb.com/v4/games` with the returned Client-ID
    and bearer token returned 200 and real game names.
- The desktop UI was then verified against the production API with an
  isolated app data/cache directory:
  - The disconnected build showed only the connection card.
  - After connecting, the Store rendered 35 PC titles with IGDB covers and
    metadata, proving the direct IGDB path and the direct `external_games`
    batch lookup worked in the release build.
  - No FastAPI catalog page/detail route was used by the Store path; the
    backend calls were status/token only.

## Decisions and risks

### Decisions

- Use the Twitch client-credentials application token, not a user OAuth token.
  The connection page authorizes IGDB for the application and stores a
  per-user connection row.
- Keep the backend catalog endpoints for compatibility; the desktop no longer
  uses them for Store browsing.
- Use process memory for the desktop token cache; no disk persistence.
- Normalize direct IGDB data in Rust so the existing Store DOM contract does
  not change.

### Risks

- The backend must be deployed before the desktop can connect. A desktop built
  against an old backend will show the connection-unavailable state.
- Per-user rows duplicate the same application token. This is intentional for
  the requested UX; it does not isolate IGDB rate limits per user.
- Twitch can revoke or rotate an application token before its reported expiry.
  The one-retry-on-401 path handles this without a loop.
- Direct IGDB requests bypass the backend’s prior rate lane. The desktop lane
  and IGDB’s own limits must be respected.
- IGDB search/count behavior is not identical to the backend’s normalized
  contract. The normalization tests must cover missing fields and absent
  counts.
- The settings page is public under `/ui`, so it must enforce the session
  itself and redirect to login without exposing tokens.
- Cached pages remain local catalog data, not credentials. The disconnected
  screen hides them until IGDB is reconnected, then the cache remains usable.

## Links

- Backend: `/mnt/krunal/WorkSpace/Code/UMUTron-WorkSpace/UmuTron-FastAPI`
- Backend IGDB client: `app/catalog/igdb.py`
- Backend catalog contract: `app/catalog/contract.py`
- Existing auth: `app/auth.py`, `app/security.py`
- Desktop Store: `crates/steamtron-desktop/ui/app.js`
- Desktop Store commands: `crates/steamtron-desktop/src/main.rs`
- SteamTron implementation commit: `dc83cfde4c`
- UmuTron-FastAPI implementation commit: `224efbf85f`
- Related backlog: `0068-umutron-parity-backlog.md` (`L-06`, `L-11`)
