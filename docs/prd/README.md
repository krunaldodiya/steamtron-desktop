# Per-feature PRDs

One PRD per feature or issue. [`docs/PRD.md`](../PRD.md) stays the product-wide
source of truth; the files here track individual work items, their acceptance
criteria, and the evidence that they were verified.

## Lifecycle

| Status | Meaning |
|---|---|
| `draft` | Problem agreed; requirements still changing |
| `active` | Implementation in progress |
| `done` | Acceptance criteria verified; evidence recorded |

Update the status and verification in the same commit as the implementation.

## Naming

`NNNN-<slug>.md` — four-digit sequence, lowercase hyphenated slug. Take the next
number from the index below and add the new row in the same commit.

Start every file from [`TEMPLATE.md`](TEMPLATE.md).

## Index

| ID | Title | Status | Updated |
|---|---|---|---|
| [0001](0001-prd-process.md) | Per-feature PRD tracking | done | 2026-11-14 |
| [0002](0002-nav-focus.md) | Restore keyboard and controller focus across routes | done | 2026-11-15 |
| [0003](0003-startup-steam-flash.md) | Remove startup Steam-installation error flash | draft | 2026-11-15 |
