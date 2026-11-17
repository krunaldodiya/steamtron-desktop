# Windows library drive discovery

- Status: done
- Date: 2026-11-16
- Areas: `crates/steamtron-desktop/src/main.rs`, Windows build

## Problem

The **Add game drive** dialog shows “No mounted internal partitions found”
on Windows even when the system disk and a data disk are present. The
`available_library_drives` command only contains a Linux implementation; the
Windows branch returns an empty list.

## Scope

- Enumerate mounted fixed and removable Windows volumes.
- Return the same JSON shape as Linux: path, label, device, filesystem, space,
  writable, registered and default.
- Include the system volume (`C:`) so the operator can choose it.
- Keep Linux behavior unchanged.

Non-goals:

- Drive formatting or partitioning.
- Network shares / UNC paths.
- WinFsp packaging.

## Requirements

- R1: Windows enumerates `A:`–`Z:` logical drives.
- R2: Include `DRIVE_FIXED` and `DRIVE_REMOVABLE`; exclude CD-ROM, RAM disk,
  unknown and unavailable roots.
- R3: Report total/free bytes using `GetDiskFreeSpaceExW`.
- R4: Report writability without leaving a probe file behind.
- R5: Compare registered drives using canonical paths.
- R6: Keep the Linux implementation untouched.

## Acceptance criteria

- [ ] Windows Add game drive lists the system disk and an attached data disk.
- [ ] A registered drive is marked registered/default after refresh.
- [ ] Read-only and CD-ROM volumes do not appear.
- [ ] Windows build compiles.
- [ ] Linux build and existing tests still pass.

## Verification

- Local Linux: `cargo test -p steamtron-desktop`.
- Windows: install the CI-built installer in the QEMU VM, open Add game drive,
  and verify both volumes.
- Record the exact disk list and screenshots in this PRD.

## Decisions and risks

- Use the already-present `windows-sys` crate instead of raw FFI.
- Include the system volume; the UI will let the operator choose whether to use
  it.
- Writability uses a temporary probe file, removed immediately.
- No test can validate Windows enumeration on the Linux host; the Windows VM is
  the acceptance environment.
