# Native Goldberg extraction and Steamless unpacking

- Status: done
- Date: 2026-11-15
- Areas: `crates/steamtron-desktop/src/fix_tools.rs`, `crates/steamtron-desktop/ui/app.js`

## Problem

Apply GoldBerg failed with "7-Zip is required to extract GoldBerg", and Apply
Steamless required Wine. The Fixes tab also showed two identical "Remove fixes"
buttons. Goldberg is a Windows DLL overlay but applying it does not need Wine;
only archive extraction was missing. Steamless was downloaded as a Wine wrapper
around a Windows CLI.

## Goal

Goldberg and Steamless no longer require external 7-Zip, Wine, or an external
Steamless download. Apply/remove work from SteamTron itself, and the Fixes tab
keeps Open fixes folder and Remove fixes together in one row, with no
enable/disable toggle.

## Non-goals

- Adding SteamStub variants beyond the engine's supported list.
- Changing how the fixes overlay is mounted at Play.

## Requirements

- R1: Goldberg `.7z` is extracted in-process with a pure-Rust 7z decoder.
- R2: Steamless uses the in-process `steamless-core` Rust engine (steamless-rs),
  which supports the x86 and x64 SteamStub variants; no Wine check, download,
  or external wrapper.
- R3: Each launcher copy is checked for SteamStub protection before unpacking.
- R4: Unpacked output replaces the fixes copy; the installed game files remain
  untouched.
- R5: The Fixes tab shows a single action row: Open fixes folder and Remove
  fixes. The duplicate Remove fixes button, the launch-overlay section, and both
  Disable/Enable fixes toggles (managed tab and legacy game-detail panel) are
  removed.
- R6: UI text no longer mentions downloading Steamless or requiring Wine.

## Acceptance criteria

- [x] Apply GoldBerg extracts `emu-win-release.7z` without 7-Zip.
- [x] Apply Steamless no longer checks for or invokes Wine.
- [x] The built-in unpacker produces the unpacked launcher in the fixes folder.
- [x] A clean/not-protected launcher returns a no-op result instead of an error.
- [x] Unsafe/unsupported launchers are skipped with a per-file result.
- [x] The Fixes tab shows Open fixes folder and Remove fixes in one row.
- [x] Fixes have no Disable/Enable toggle in either UI surface; Play applies a
  fixes folder whenever it contains files.
- [x] Existing apply/remove state and manifests remain compatible.

## Verification

- `cargo check -p steamtron-desktop` passed.
- Downloaded the pinned GoldBerg asset (`11,324,621` bytes, expected SHA-256)
  and extracted it with `sevenz-rust`; `release/regular/x64/steam_api64.dll`
  was produced.
- Tested `steamless-core` against `MilesMorales.exe` (PE64): the engine parsed
  it as 64-bit and reported no matching SteamStub variant, so Apply now returns
  "No SteamStub protection was found" instead of a failure.
- `node --check crates/steamtron-desktop/ui/app.js` passed.
- Dialog UI suite 73/73, base 34/34, auth 13/13.

## Decisions and risks

- `steamless-core` (steamless-rs) supports variants 1.0, 2.0/2.1, 3.0, and
  3.1/3.1.2 on x86 and x64. No Wine or .NET runtime is involved.
- The engine is GPL-3.0; it is linked as a source dependency at a pinned
  revision.
- Goldberg remains a Windows emulator overlay; it is not a Linux binary. It is
  applied as files consumed by the game under Proton, so Wine is not needed to
  apply it.
- `fixes_enabled` stays in the library format for compatibility, but the UI no
  longer writes it; Play layers a fixes folder whenever it contains files.
- The 7z decoder is pure Rust and runs in-process, so extraction works on both
  Linux and Windows without a system tool.

## Links

- Related: `docs/prd/0018-steamless-operations.md`
