# Use the local Store catalogue cache

- Status: done
- Date: 2026-11-15
- Areas: `crates/steamtron-desktop/ui/app.js`

## Problem

The Rust backend already stored the Store genre list and every catalogue page
under `~/.cache/steamtron/catalog`, and exposed `store_cached_genres` and
`store_cached_catalog`. The UI never called those commands. Every Store visit
called `store_genres` and `store_catalog`.

When the API request failed, the Store showed a raw
`error sending request for url (...)` message, an empty genre list, and
"Store unavailable", even though the cache held a complete genre list and the
first two popular pages. The background warm-up made this visible before the
user even opened the Store.

## Goal

Paint the Store from the local cache first. Call the API only for pages that are
not cached. Keep the Store usable when the API is offline, and show an error
only when there is no cached content to show.

## Non-goals

- Adding a manual cache-clear or force-refresh control.
- Changing the IGDB-to-Steam resolution cache.
- Changing the Store API or its authentication.

## Requirements

- R1: Load genres from `store_cached_genres` before trying the network.
- R2: Load each catalogue page from `store_cached_catalog` before trying the
  network.
- R3: Only fetch uncached server pages from `store_catalog`.
- R4: A network failure keeps cached cards and genres on screen; the raw error
  is shown only when the buffer is empty.
- R5: A Store visit after a cache miss retries the network instead of leaving
  the old error on screen.
- R6: Filtering by a query, genre, or sort that has no cache still uses the API.

## Acceptance criteria

- [x] With cached genres and catalogue pages, warm-up and Store entry issue no
  `store_genres` or `store_catalog` requests.
- [x] The cached genre list and cached cards paint on the Store screen.
- [x] With no cache, the Store falls back to `store_genres` and `store_catalog`
  and renders the result.
- [x] A failed warm-up with no cache does not prevent a retry when the Store is
  opened.
- [x] Cached content is filtered through the same PC/Steam-availability checks
  as network content.

## Verification

- jsdom harness `/tmp/steamtron-nav-test/test.mjs` (33 checks) passed in the
  default cache-hit mode. It asserts that `store_cached_genres` and
  `store_cached_catalog` are called and that neither `store_genres` nor
  `store_catalog` is called during warm-up, then checks the cached genre
  `Adventure` and both cached cards render.
- The same harness with `STORE_CACHE_MISS=1` passed and asserts the API fallback
  is used when the cache returns nothing.
- The harness also passed with `LIST_GAMES_FAIL=1`.
- Live build: the Store painted the cached genre list and 34 cached PC titles
  with no error banner, while the API was not called for cached pages.
- `node --check crates/steamtron-desktop/ui/app.js` passed.

## Decisions and risks

- Cache-first means a cached page is not silently refreshed on every launch.
  New queries, genres, sorts, and pages still fetch from the API and update the
  cache.
- `storeRefreshing` remains part of the UI state for pages that mix cached and
  freshly fetched server pages.

## Links

- Related: `docs/prd/0002-nav-focus.md`, `docs/prd/0007-detail-focus.md`
- Cache format: `crates/steamtron-desktop/src/main.rs` (`catalog_cache_path`,
  `store_cached_genres`, `store_cached_catalog`)
