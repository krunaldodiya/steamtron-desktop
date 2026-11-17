# Do not offer backend install without Steam

- Status: done
- Date: 2026-11-17
- Areas: `crates/steamtron-doctor`, `crates/steamtron-desktop/ui`

## Problem

On Windows, Doctor showed:

```text
Steam installation — Steam installation was not found
BetterSteamTools — Steam was not found   [Install BetterSteamTools]
```

The install action was offered even though BetterSteamTools installs into the
Steam root and Steam was absent. Clicking it could only fail. The same action
was also exposed on Windows even though automatic BetterSteamTools installation
is not implemented yet.

## Goal

Doctor never offers an unlock-backend install action when Steam is missing, and
the Windows backend row does not pretend an automatic install is available when
it is not implemented.

## Non-goals

- Implementing the BetterSteamTools installer.
- Installing Steam.
- Changing Linux backend installation.

## Requirements

- R1: When Steam discovery fails, the `unlock-backend` check has no repair
  action.
- R2: The backend detail tells the user to install Steam first.
- R3: On Windows, no automatic BetterSteamTools install action is exposed.
- R4: The UI derives repair buttons from the check's `repair` field, not from
  the check id.
- R5: `repair_steam_library` still refuses before launching an installer when
  Steam is missing.

## Acceptance criteria

- [x] A missing Steam root produces no Install BetterSteamTools/SLSsteam button.
- [x] The backend row still reports its error and explains that Steam is needed.
- [x] Windows Doctor remains honest that automatic BetterSteamTools install is
  not available in this build.
- [x] Linux install actions remain available when Steam exists.

## Verification

- `cargo test -p steamtron-doctor` — 2 passed.
- `cargo check -p steamtron-desktop` — pass.
- `node --check crates/steamtron-desktop/ui/app.js` — pass.
- Release build and install to `~/.local/bin/steamtron-desktop` completed;
  SHA-256 matched the build.
- The Windows VM screenshot path is covered by the shared Doctor contract:
  missing Steam leaves `repair = null`, and the UI now only renders an install
  button from `check.repair`. A Windows-built binary was not available in this
  session, so the Windows UI was not re-exercised.

## Decisions and risks

- BetterSteamTools installation remains unimplemented. Hiding the button is the
  honest behavior until an installer exists; it does not satisfy the long-term
  automatic-helper requirement.
- The global Repair button can still be clicked without Steam, but the command
  returns the existing clear error. Only per-check install actions were removed.

## Links

- `docs/prd/0045-backend-install-requires-steam.md`
- `crates/steamtron-doctor/src/lib.rs`
- `crates/steamtron-desktop/ui/app.js`
