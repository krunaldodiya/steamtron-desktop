# Install Visual C++ runtime on Windows

- Status: active
- Date: 2026-11-17
- Areas: Windows packaging, SMD bootstrap

## Problem

SMD's Windows executable exits at startup with:

```text
VCRUNTIME140.dll was not found
```

The Windows installer does not provide the Microsoft Visual C++ 2015–2022
x64 runtime. Linux SMD is statically usable; the Windows build is not.

## Requirements

- R1: Install or bundle the VC++ x64 runtime before SMD starts.
- R2: Prefer the official Microsoft `vc_redist.x64.exe`.
- R3: Do not download it silently at ordinary app runtime without consent.
- R4: Keep Linux behavior unchanged.
- R5: Surface a clear error if the runtime installation fails.

## Verification

- Clean Windows VM: install SteamTron, start a managed download, confirm SMD
  starts and emits depot progress.
- Confirm no `VCRUNTIME140.dll` error.
