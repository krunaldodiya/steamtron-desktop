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
has one Remove fixes action.

## Non-goals

- Adding native x64 SteamStub support that the Rust unpacker does not provide.
- Changing how the fixes overlay is mounted at Play.

## Requirements

- R1: Goldberg `.7z` is extracted in-process with a pure-Rust 7z decoder.
- R2: Steamless uses the in-process `libsteamless` unpacker; no Wine check,
  download, or external wrapper.
- R3: Each launcher copy is checked for SteamStub protection before unpacking.
- R4: Unpacked output replaces the fixes copy; the installed game files remain
  untouched.
- R5: The duplicate "Remove fixes" button in the SteamTron launch overlay is
  removed; the single action stays in Current fixes location.
- R6: UI text no longer mentions downloading Steamless or requiring Wine.

## Acceptance criteria

- [x] Apply GoldBerg extracts `emu-win-release.7z` without 7-Zip.
- [x] Apply Steamless no longer checks for or invokes Wine.
- [x] The built-in unpacker produces the unpacked launcher in the fixes folder.
- [x] Unsafe/unsupported launchers are skipped with a per-file result.
- [x] The Fixes tab shows one Remove fixes button.
- [x] Existing apply/remove state and manifests remain compatible.

## Verification

- `cargo check -p steamtron-desktop` passed.
- Downloaded the pinned GoldBerg asset (`11,324,621` bytes, expected SHA-256)
  and extracted it with `sevenz-rust`; `release/regular/x64/steam_api64.dll`
  was produced.
- `node --check crates/steamtron-desktop/ui/app.js` passed.
- Dialog UI suite 73/73, base 34/34, auth 13/13.

## Decisions and risks

- `libsteamless` currently implements the x86 SteamStub variants only. A
  64-bit SteamStub executable reports an unpack failure instead of falling back
  to Wine; this is intentional because Wine is no longer a requirement.
- Goldberg remains a Windows emulator overlay; it is not a Linux binary. It is
  applied as files consumed by the game under Proton, so Wine is not needed to
  apply it.
- The 7z decoder is pure Rust and runs in-process, so extraction works on both
  Linux and Windows without a system tool.

## Links

- Related: `docs/prd/0018-steamless-operations.md`
