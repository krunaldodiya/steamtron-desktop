# Agent rules

## Never assume, bluff, or hallucinate — mandatory

Every claim about behavior must be backed by evidence actually produced in the
current session.

- Do not report a fix as done, working, verified, or tested unless the specific
  command, test, or UI interaction was run and its output was observed.
- Distinguish code inspection from runtime proof. If only the code was read,
  say that; do not describe it as runtime verification.
- Use `[INFERENCE]` for conclusions that are reasoned but not directly tested.
- If the surface cannot be exercised with available tools, say so explicitly
  and report exactly what was verified instead.
- UI changes require verification against the rebuilt, installed binary. A
  stale running process or an earlier build is not evidence.
- Re-run the relevant scenario after the final change. Do not claim success
  from a previous run, a sibling command, or an assumption about what the code
  should do.
- Never fabricate command output, test results, screenshots, or dialog
  behavior.

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
- If work was started on a feature or work branch, merge it into `main` locally,
  push `main`, and verify `origin/main` contains the commit. Never leave
  completed work only on a branch at `origin`.
- Never leave the working tree dirty or the branch ahead of `origin`.
- Verify before reporting:
  `git status --porcelain` prints nothing and
  `git rev-list --count origin/main..HEAD` prints `0`.
- If the push fails, say so explicitly and retry; never report success while
  the commits are local-only.
- Commit and push manually; there is no autosave timing service. Never create
  or push `autosave/*` branches.

## Build and install only on request — mandatory

Builds and installs are slow and are not part of every change.

- Do **not** build or install after each change. Accumulate work and build only
  when the user asks, or after a substantial batch of changes **and** with the
  user's explicit permission.
- Always ask before starting a build/install unless the user just asked for one.
- When a build is approved, commit the source first, build the release bundle,
  install it, and verify the installed file timestamp/hash matches the new build.
- Until a build is approved and installed, report only code-level verification
  (compile, tests, diffs). Never claim a UI change was verified against the
  running app while the installed binary predates it.
- Build artifacts stay out of git.
