# Keep the DLC size column on one line

- Status: done
- Date: 2026-11-17
- Areas: `crates/steamtron-desktop/ui/style.css`

## Problem

The DLC size column was 80px wide, so `984.3 MiB` wrapped onto two lines while
`12.1 GiB` did not.

## Goal

The size column stays on one line and is wide enough for both MiB and GiB values.

## Requirements

- R1: Last DLC table column is at least 104px.
- R2: Size cells use `white-space: nowrap`.
- R3: No other DLC table behavior changes.

## Verification

- CSS rule updated to `width:104px; min-width:104px; white-space:nowrap`.
- 0.1.25 Linux and Windows release builds succeeded.

## Decisions and risks

- A fixed minimum keeps the column aligned without changing table layout logic.
