# Install maintained BetterSteamTools

- Status: active
- Date: 2026-11-18
- Areas: `crates/steamtron-doctor/scripts/windows-install-bettersteamtools.ps1`

## Problem

The Windows backend installer downloaded OpenSteamTool 1.4.8 (2026-06-13).
lua.tools ships **BetterSteamTools**, a maintained fork of that project, at
v1.0.9 (2026-11-13). On a Steam client built 2026-11-12 the older DLL could not
apply the Lua manifests: Steam showed `All Games (0)` while
`config/lua/*.lua` contained the addappid entries.

## Goal

Install the maintained v1.0.9 release and verify its checksum.

## Non-goals

- Installing Millennium or the LuaTools plugin.
- Changing how SteamTron generates Lua manifests.

## Requirements

- R1: Download `OpenSteamTool-v1.0.9-Release.zip` from `madoiscool/BetterSteamTools`.
- R2: Pin the observed SHA-256 `af1affe6…d366f`.
- R3: Keep copying `OpenSteamTool.dll`, `dwmapi.dll`, `xinput1_4.dll`.

## Acceptance criteria

- [ ] The release archive hash matches the pinned value.
- [ ] The extracted DLLs are the v1.0.9 files (OpenSteamTool.dll 1,742,848 B).

## Verification

- Downloaded the archive and observed the hash above; extracted
  `OpenSteamTool.dll` = 1,742,848 bytes, `dwmapi.dll` = 109,568,
  `xinput1_4.dll` = 122,880.
- Manually installed those files into Steam; BetterSteamTools generated
  `opensteamtool/pattern/steamclient/*.toml`, proving the v1.0.9 DLL initializes
  against Steam client `1794536281`.

## Decisions and risks

- The fork keeps the upstream asset name and DLL names, so the installer only
  changes the URL/hash and log labels.
- Core BetterSteamTools grants licenses; Steam's library UI still depends on the
  LuaTools Millennium plugin, which this installer does not add.

## Links

- Related: `0090-minimal-doctor`, `0091-library-sync`.
