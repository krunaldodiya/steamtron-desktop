# Hide helper console windows on Windows

- Status: active
- Date: 2026-11-16
- Areas: `crates/steamtron-core/src/process.rs`, desktop/core/doctor command call sites

## Problem

Launching SteamTron on Windows briefly opens and closes 10–20 console windows.
The GUI application starts helper processes such as `reg`, `tasklist`,
`taskkill` and PowerShell. `std::process::Command` gives each child a console
window unless `CREATE_NO_WINDOW` is set.

## Goal

Every helper process started by the app is invisible on Windows. Linux command
behavior is unchanged.

## Requirements

- R1: Use one shared command constructor with `CREATE_NO_WINDOW` on Windows.
- R2: Route every desktop/core/doctor child-process spawn through it.
- R3: Keep Linux commands unchanged.
- R4: Do not hide user-requested terminals such as the diagnostic log opener.

## Acceptance criteria

- [ ] Launching SteamTron produces no console flashes.
- [ ] Registry, tasklist, taskkill, PowerShell and helper operations still work.
- [ ] Linux build/tests unchanged.
- [ ] Windows build compiles.

## Verification

- `cargo check -p steamtron-desktop -p steamtron-doctor`
- Linux tests.
- Windows VM: launch, open Store, Sign in, Doctor, drive refresh and confirm no
  console flashes.

## Decisions and risks

- The constructor is a no-op on non-Windows targets.
- The diagnostic-log terminal opener is a user-requested console and remains
  visible.
