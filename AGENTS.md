# Agent rules

## Every feature or issue gets a PRD — mandatory

No implementation work starts before its PRD exists.

- Create `docs/prd/NNNN-<slug>.md` from `docs/prd/TEMPLATE.md`; take the next
  number from the index in `docs/prd/README.md`.
- Record: problem, scope/non-goals, affected crates or areas, requirements,
  acceptance criteria, verification, decisions/risks, and status.
- Update the PRD in the same commit as the implementation:
  `draft` → `active` → `done`, with the verification evidence.
- Small fixes use the same file with only the sections that apply. Never skip
  the PRD because the change looks trivial.
- `docs/PRD.md` remains the product-wide source of truth. Per-item PRDs add
  detail; they do not override it.
- Existing documents are grandfathered; the rule applies to work started from
  2026-11-14 onward.

## Commit and push after every change — mandatory

Local work that is not on `origin` does not exist. This rule overrides any
preference for batching commits or keeping a clean history.

- Every change ends with a commit and a push to `origin/main` in the same turn,
  before reporting the work as done.
- Never leave the working tree dirty or the branch ahead of `origin`.
- Verify before reporting:
  `git status --porcelain` prints nothing and
  `git rev-list --count origin/main..HEAD` prints `0`.
- If the push fails, say so explicitly and retry; never report success while
  the commits are local-only.
- Interim snapshots of unfinished work go to `autosave/<host>`
  (`~/.local/bin/git-autosync.sh`, timer `git-autosync.timer`); they are a
  safety net, not a substitute for real commits.
