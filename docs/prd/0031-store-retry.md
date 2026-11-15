# Retry Store requests with a fresh client

- Status: done
- Date: 2026-11-15
- Areas: `crates/steamtron-desktop/src/main.rs`

## Problem

Store search failed with:

```
error sending request for url (https://umutron-fastapi.vercel.app/v1/games?...)
```

The API was reachable: `curl` and a standalone `reqwest` 0.12 request with the
saved access token both returned HTTP 200 for `q=cricket+26`. The failure was in
the long-lived app HTTP client/connection pool, not the catalogue endpoint.

## Goal

A stale pooled connection does not permanently break Store search; one request
failure retries with a fresh client and surfaces a user-readable message.

## Requirements

- R1: Keep the shared Store client for normal requests.
- R2: On a send error, wait briefly and retry once with a new client.
- R3: The retry reports `Could not reach the Store. Check your connection and
  retry.` instead of the raw reqwest URL error.
- R4: 401/403 still report the existing sign-in error.

## Acceptance criteria

- [ ] A transient send failure retries and succeeds when the API is reachable.
- [ ] Repeating the search does not require an app restart.
- [ ] Auth failures keep the existing sign-in message.

## Verification

- `curl` with the saved token returned HTTP 200 and 841 bytes.
- A standalone reqwest 0.12 client returned HTTP 200.
- `cargo check` passed.
- After the retry change, the installed app's Store search for `cricket` returned
  results normally.

## Decisions and risks

- The app uses a fresh client only for the retry; normal traffic keeps pooling.
- No automatic retry loop is added, so a genuinely offline Store does not spam.

## Links

- Implementation: `crates/steamtron-desktop/src/main.rs`
