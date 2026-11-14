# Per-feature PRD tracking

- Status: done
- Date: 2026-11-14
- Areas: docs, agent workflow

## Problem

Features and issues were tracked only in chat sessions and commit messages.
Requirements, acceptance criteria, and verification evidence were not durable,
so later sessions had to reconstruct intent from transcripts and diffs.

## Goal

Every feature or issue has a PRD under `docs/prd/`, created before implementation
and updated in the same commit as the code.

## Non-goals

- Replacing `docs/PRD.md`, which remains the product-wide source of truth.
- Migrating or renumbering the existing design and migration documents.

## Requirements

- R1: `AGENTS.md` requires a PRD before implementation starts.
- R2: PRDs use a fixed template with problem, requirements, acceptance criteria,
  verification, decisions, and status.
- R3: `docs/prd/README.md` indexes every PRD and defines the lifecycle.
- R4: The PRD and the implementation land in the same commit.

## Acceptance criteria

- [x] `AGENTS.md` contains the mandatory PRD rule.
- [x] `docs/prd/TEMPLATE.md` defines the required sections.
- [x] `docs/prd/README.md` defines statuses, naming, and the index.
- [x] This file is listed in the index.

## Verification

- `git status --porcelain` is empty after the commit and push.
- `git rev-list --count origin/main..HEAD` is `0`.
- `docs/prd/README.md` links to this file; this file follows the template.

## Decisions and risks

- Four-digit sequence with a manual index keeps IDs stable and reviewable.
  A generated tracker would add tooling for a low-churn document set.
- PRDs live next to the code rather than in an external tracker so they travel
  with the repository and are readable offline.
- Risk: contributors may treat the PRD as write-once. The lifecycle and the
  same-commit rule require it to be updated when work completes.

## Links

- [`AGENTS.md`](../../AGENTS.md)
- [`docs/prd/TEMPLATE.md`](TEMPLATE.md)
