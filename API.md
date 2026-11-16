# SteamTron API

This document lists the endpoints SteamTron calls, their authentication, and the
data each one returns. It separates first-party SteamTron endpoints from direct
upstream providers.

## Base URLs

The desktop resolves the first-party API base in this order:

1. `STEAMTRON_API_BASE` environment variable.
2. `api_base` in `~/.config/steamtron/config.json`
   (`%APPDATA%\SteamTron\config.json` on Windows).
3. `https://umutron-fastapi.vercel.app`.

The backend is the private SteamTron API repository:

- Current source: `UmuTron-FastAPI`, being renamed to `steamtron`
- Current implementation: FastAPI; the planned fullstack replacement is
  Next.js and must preserve this desktop contract
- Health: `GET /health`
- Interactive schema: `GET /docs`

The current fallback hostname still contains the old `umutron-fastapi` name.
It will move to the SteamTron deployment after the repository/domain migration.
Desktop builds can point at the new origin immediately with
`STEAMTRON_API_BASE` or `~/.config/steamtron/config.json`.

All first-party endpoints except the browser login/connect pages require a
bearer token.

```http
Authorization: Bearer <steamtron-access-token>
Accept: application/json
```

## Authentication

SteamTron uses OAuth 2.0 authorization code + PKCE through the system browser.

### `GET /oauth/authorize`

Browser endpoint used by the desktop login flow.

Query parameters:

| Name | Value |
|---|---|
| `response_type` | `code` |
| `client_id` | `umutron-desktop` |
| `redirect_uri` | `http://127.0.0.1:<ephemeral-port>/callback` |
| `code_challenge` | S256 PKCE challenge |
| `code_challenge_method` | `S256` |
| `state` | Random 32-character state |

The browser redirects to the loopback callback with `code` and `state`, or with
an `error` query parameter.

### `POST /oauth/token`

Form-encoded token exchange.

| Field | Value |
|---|---|
| `grant_type` | `authorization_code` |
| `code` | Authorization code |
| `client_id` | `umutron-desktop` |
| `redirect_uri` | Same loopback URI used for authorization |
| `code_verifier` | Original PKCE verifier |

Response:

```json
{
  "access_token": "<jwt>",
  "token_type": "Bearer",
  "expires_in": 604800,
  "user": {
    "id": "<user-id>",
    "email": "user@example.com"
  }
}
```

The desktop stores the token with owner-only permissions at:

```text
~/.local/share/steamtron/api-token.json
%LOCALAPPDATA%\SteamTron\api-token.json
```

## IGDB connection

The Store itself calls IGDB directly. These endpoints provide the per-user
Twitch/IGDB application token and connection state.

### `GET /v1/igdb/status`

Returns state without exposing the access token.

```json
{
  "configured": true,
  "connected": true,
  "expires_at": 1800146342,
  "connect_url": "https://umutron-fastapi.vercel.app/ui/settings"
}
```

| Field | Meaning |
|---|---|
| `configured` | Server has `TWITCH_CLIENT_ID` and `TWITCH_CLIENT_SECRET` |
| `connected` | The user has a usable token row |
| `expires_at` | Unix seconds; `0` when disconnected |
| `connect_url` | Browser settings page for connecting IGDB |

The desktop treats this as a cold-session/recovery endpoint. A cached,
non-expired token answers without a backend request.

### `GET /v1/igdb/token`

Returns the token used by direct IGDB requests.

```json
{
  "client_id": "<twitch-client-id>",
  "access_token": "<twitch-app-access-token>",
  "expires_at": 1800146342,
  "base_url": "https://api.igdb.com/v4"
}
```

Response headers include `Cache-Control: no-store`.

Errors:

| HTTP | Code | Meaning |
|---|---|---|
| 401 | `UNAUTHORIZED` | Desktop session is missing or expired |
| 409 | `IGDB_NOT_CONNECTED` | No token row; open the settings page |
| 409 | `IGDB_NOT_CONFIGURED` | Server has no Twitch credentials |
| 503 | `IGDB_TOKEN_UNAVAILABLE` | Twitch/API refresh failed |

The desktop refreshes within 120 seconds of expiry. The backend refreshes
within 300 seconds. A direct IGDB `401` invalidates the desktop cache and
retries once.

The token is a Twitch **application** token from the client-credentials grant,
not a personal Twitch user token.

### `POST /v1/igdb/connect`

Creates or replaces the current user's IGDB token. No request body.

Response is the same shape as `GET /v1/igdb/token`. The browser settings page
uses the same operation internally.

### `GET /ui/settings`

Browser page for connecting or refreshing IGDB. Requires a browser session
cookie. The desktop opens this page from the Store connection card.

## Manifest provider

### `GET /api/download/{appid}`

Requested by the managed-game manifest provider when adding a game or resolving
a manifest bundle.

Query parameters:

| Name | Value |
|---|---|
| `file_type` | `manifest` |

Headers:

```http
Authorization: Bearer <token>
Accept: application/zip
```

Response: ZIP archive containing the Lua manifest and depot manifest files
understood by `steamtron-library`.

> The current FastAPI implementation does not include this route; the deployed
> service currently returns 404. The desktop contract remains listed
> here because the API-provider add path expects it. Until the backend provides
> it, use the public provider or offline ZIP path. See
> `crates/steamtron-library/src/lib.rs` (`download_from_api`).

## Direct IGDB API

After obtaining the token above, the desktop calls the official host directly.
No FastAPI catalog route is involved in Store page loading.

Base URL:

```text
https://api.igdb.com/v4
```

Headers:

```http
Client-ID: <client_id from /v1/igdb/token>
Authorization: Bearer <access_token>
Accept: application/json
Content-Type: text/plain
```

APICalypse queries are sent as the request body.

### `POST /games`

Used for Store pages and searches.

- Page size: 36 items.
- Request limit: 37 items (`has_next` uses the extra row).
- Server-side filters:
  - Search: `search "<query>";`
  - Genre: `where genres = <id>;`
  - PC platforms: `where platforms = (6,14,3);`
  - Combined clauses use `&`.
- Fields include name, summary, storyline, release date, genres, cover,
  artwork, companies, version title, game type, and platforms.
- No `games/count` request is made.

### `POST /genres`

Returns the genre list. The desktop caches it in the local catalogue cache and
only refreshes it when that cache is absent.

### `POST /external_games`

Used to map IGDB game IDs to Steam App IDs for the current page.

```text
fields game,external_game_source.name,uid;
where game = (<id>,<id>,... ) & external_game_source = 1;
limit 100;
```

`external_game_source = 1` is Steam.

### `POST /games/{id}`

Not used; detail reads use the IGDB `games` endpoint with `where id = <id>`.

## Other direct upstream APIs

These are not first-party API routes and do not require a SteamTron account
token.

| Provider | Endpoint | Purpose |
|---|---|---|
| Steam Store | `GET https://store.steampowered.com/api/appdetails?appids=<appid>&l=english` | Game metadata and DLC discovery |
| Steam Store | `GET https://store.steampowered.com/api/storesearch/?term=<title>&l=english&cc=US` | Title-to-App-ID matching for Store entries |
| SteamCMD API | `GET https://api.steamcmd.net/v1/info/<appid>` | Download/install size metadata |
| ProtonDB | `GET https://www.protondb.com/api/v1/reports/summaries/<appid>.json` | Compatibility tier |
| Steam artwork | `GET https://api.steampowered.com/IStoreBrowseService/GetItems/v1/` | Modern cover/hero assets |
| Steam artwork | `GET https://shared.akamai.steamstatic.com/store_item_assets/` | Modern asset CDN |
| Steam artwork | `GET https://cdn.akamai.steamstatic.com/steam/apps/` | Legacy asset fallback |
| SteamML archive | `GET https://pub-5b6d3b7c03fd4ac1afb5bd3017850e20.r2.dev/<appid>.zip` | Public fallback manifest bundle |
| Twitch OAuth | `POST https://id.twitch.tv/oauth2/token` | Backend-only application-token issuance |

## Legacy first-party catalog endpoints

These remain available on the backend for compatibility, but the current
desktop Store does not use them:

- `GET /v1/genres`
- `GET /v1/games?q=&genre=&page=&sort=`
- `GET /v1/games/{igdb_id}`

See the SteamTron API source for their response contract.

## Error format

First-party API errors use:

```json
{
  "error": {
    "code": "IGDB_NOT_CONNECTED",
    "message": "Connect IGDB in the SteamTron API settings before browsing the Store."
  }
}
```

The desktop maps `IGDB_NOT_CONNECTED` to the reconnect card and
`IGDB_NOT_CONFIGURED` to the server-configuration card.

## Security notes

- Never log or paste `access_token` values.
- Never put tokens in URLs.
- The desktop keeps the IGDB token in process memory only.
- The FastAPI service stores per-user IGDB connection rows in Turso/SQLite.
- IGDB and Twitch requests use fixed HTTPS hosts. No user-controlled endpoint
  is accepted by the token client.
- The backend settings page requires a session cookie even though `/ui` is
  outside the bearer-token middleware.
