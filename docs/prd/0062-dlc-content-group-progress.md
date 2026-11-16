# DLC content-group progress

- Status: done
- Date: 2026-11-16
- Areas: `crates/steamtron-desktop/src/dlc.rs`

## Problem

The DLC manager showed overall percentage, but its Content groups row stayed at
`Waiting`, `0%`, `0 B / 110.7 MiB` until the download finished. Only the DLC
row was updated after installation.

## Requirements

- R1: Mark the DLC row `downloading` before the child downloader starts.
- R2: Mirror overall DLC progress into its row's percent and completed bytes.
- R3: Mark the row `done` after files are installed.
- R4: Apply to both DDM and SMD paths.

## Verification

- `cargo check` passes.
- Desktop tests pass.
- The row update is mirrored from the same progress value as the overall bar.
