---
name: next
description: Where things stand (cycle, plans, uncommitted and unpushed work, CI, what is due) and the one command to run next.
disable-model-invocation: true
---

# /next — where we are, and what to run

Read-only: it changes nothing, it orients the human. Gather in parallel:

- `AGENTS.md` "Current state" and the open section of `docs/ROADMAP.md`
  (its in-list against what landed);
- `docs/plans/*.md` but `TEMPLATE.md`: each plan's unchecked steps with
  their grades, and whether a plan file is untracked;
- `docs/ideas/*.md`: each idea's `Status` and any unanswered round;
- `git status --short`, `git log --oneline origin/main..main | wc -l`
  (unpushed), `git describe --tags --abbrev=0` and the commits since it;
- `gh run list --branch main --limit 3` (skip it quietly if `gh` fails);
- `docs/BACKLOG.md`: only its line count, and any line naming the open
  cycle.

Reply in one screen, in this shape:

```
Cycle   C4, the web: 2 of 5 in-list lines landed
Plans   c4: step 3 next ([2] the worker), 6 left
Ideas   spacemouse-support: parked after C4
Tree    clean | 3 files staged by another session | plan file untracked
Git     12 commits unpushed; last tag c3; CI on main green (2 h ago)
Due     push; tag c4 after /close-cycle; nothing to retire

→ /work c4 step 3-4   both [2], the worker then its cancel; Sonnet fits
  or: /retire-plan …  /idea …   (one line each, why they lose)
```

The `→` line is one command with its arguments and a reason, and the
model its grade suggests ([1]/[2] Sonnet, [3] Opus). A plan step that
waits on an `⚠ OPEN:` or an Arris ask is named as blocked, with what
unblocks it. Staged or modified files you did not make are another
session's: say so, and recommend nothing that would commit them.
