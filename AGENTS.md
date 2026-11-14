# Agent rules

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
