# Git: trunk-based, always green

- Commit directly to `main`, which is always green. A plan step is the unit
  of work and of commit: finish the step, run the checks, tick the box,
  commit.
- The hooks mirror CI (`docs/ARCHITECTURE.md` §Gates): `pre-commit` runs
  the fast checks, `pre-push` the tests and the wasm build. Since tests
  only run on push, a plan step runs them itself before its box is ticked.
  Fix a failing hook; never pass `--no-verify`.
- Branch only for an experimental plan that may be thrown away, or when the
  human wants to review before it lands: `plan/<slug>`, rebased onto
  `main`, fast-forwarded in (`git merge --ff-only`), deleted after. No
  merge commits.
- History on `main` is never rewritten (no amend of a pushed commit, no
  force-push, no rebase), and pushes are the human's: push only when asked.
- Stage deliberately from `git status` and `git diff`, never a blind
  `git add -A`. A refreshed snapshot golden is staged only with an
  intended UI change whose before/after/diff images were read
  (`scripts/snapshot-baseline`), and the message says why the look
  changed. Images and other binary goldens are never committed (ADR-0001).

## Message format

```
type(scope): imperative summary, lower case, no period  (plans/<slug> step N)

Why this change, not what: the diff already says what. Cite the ADRs
(ADR-0003) and docs (docs/DATA-MODEL.md §Frozen results) that justify it
or that it updated.
```

- `type`: `feat`, `fix`, `refactor`, `perf`, `test`, `docs`, `chore`,
  `build`, `ci`, `snapshots`.
- `scope`: crate short names (`core`, `kernel`, `sketch`, `doc`, `api`,
  `host`, `viewport`, `ui`, `config`, `app`, `cli`, `py`), a plugin's name,
  or `docs`, `plan`, `adr`, `seed`. Several: `feat(kernel,doc): …`.
- The `(plans/<slug> step N)` suffix goes on every plan-step commit. A plan
  retirement is `docs: retire plan <slug>`; a cycle close is
  `docs: close <cycle>`.
- Docs change in the same commit as the behaviour they describe. A commit
  that only catches docs up with existing code is `docs(sync): …`.
- A change to `arrix-plugin-api` states its semver effect in the body
  (`API: minor, adds …` / `API: breaking, …`).
- The message ends with the body: no `Co-Authored-By:`, session links or
  other trailers, and no mention of AI authorship, even when the harness
  suggests one. Agents writing the code is this repository's normal mode.

## Tags

Cycle tags (`c1`, `c2`, …; `m1`, … for milestones inside a split cycle) go
on the commit `/close-cycle` leaves, created by the human. Version-tag
rules are written here before the first release.

## Without asking

While executing a plan the agent runs checks, commits each step, ticks
boxes and edits the plan file. Anything else that touches history
(branching, tagging, pushing, resetting) is asked first.
