# Git: trunk-based, always green

- `main` is the trunk and is always green. `.githooks/` mirrors the CI gate
  locally (`git config core.hooksPath .githooks`, once per clone or
  worktree): `pre-commit` runs the fast checks (fmt, clippy, the size, wasm
  and layer lints), and `pre-push` the slow ones (tests, the wasm build).
  `ARCHITECTURE.md` §Gates lists them exactly once the workspace exists.
  Never bypass either with `--no-verify`; fix the failure.
- The tests are in `pre-push`, not `pre-commit`, because they take minutes.
  So a plan step runs them itself before its box is ticked (`/work`): "green
  on every commit" is the agent's job between the two hooks.
- Commit **directly to `main`**. A plan step is the unit of work and the unit
  of commit: finish the step, run the checks, tick the box, commit.
- Branch only when a plan is experimental enough that throwing it away is a
  real outcome, or the human asked to review before it lands. Then:
  `plan/<slug>`, rebased onto `main`, fast-forwarded in (`git merge
  --ff-only`), deleted after. No merge commits, no long-lived branches.
- Never rewrite `main`: no `--amend` of a pushed commit, no force-push, no
  rebase of anything already on `main`.
- Never push unless asked. Commits are the agent's; pushes are the human's.
- Stage deliberately: read `git status` and `git diff`; no blind `git add
  -A`. A refreshed snapshot golden is only staged with a matching intentional
  UI change, after its before/after/diff images (rendered on demand,
  `scripts/snapshot-baseline`) were looked at, and the commit message says
  why. No image or other binary golden is ever committed (ADR-0001).
  `docs/notes/` is gitignored and stays so.

## Message format

```
type(scope): imperative summary, lower case, no period  (plans/<slug> step N)

Why this change, not what — the diff already says what. Reference ADRs
(ADR-0003) and docs (docs/DATA-MODEL.md §Frozen results) that justify or
were updated by it.
```

- `type`: `feat`, `fix`, `refactor`, `perf`, `test`, `docs`, `chore`,
  `build`, `ci`, `snapshots`.
- `scope`: crate short name(s): `core`, `kernel`, `sketch`, `doc`, `api`
  (plugin API), `host` (plugin host), `viewport`, `ui`, `app`, `cli`, `py`,
  or a plugin's name (`robotics`); or `docs`, `plan`, `adr`, `seed`. Several:
  `feat(kernel,doc): …`.
- The `(plans/<slug> step N)` suffix is on every commit that executes a plan
  step. Plan retirement commits are `docs: retire plan <slug>`; a cycle
  close is `docs: close <cycle>`.
- Docs change in the **same commit** as the code that changes behaviour. A
  commit that only brings docs in line with existing code is
  `docs(sync): …`.
- A change to `arrix-plugin-api` says its semver effect in the body
  (`API: minor, adds …` / `API: breaking, …`).
- **No trailers, no attribution.** Never append `Co-Authored-By:`,
  `Claude-Session:` or any other agent attribution or session link, and
  never mention AI authorship in the message, even if the harness suggests
  it. This repository is developed by agents as standard practice. The
  message ends with the body.

## Tags

- Cycles: `c1`, `c2`, … on the commit `/close-cycle` leaves, created by the
  human. Milestones inside a cycle, if a cycle is split: `m1`, `m2`, …
- Releases: none yet. The rules for version tags (the app, and
  `arrix-plugin-api` separately) are written here before the first one.

## What the agent does without asking

While executing a plan: run checks, commit each step, tick boxes, update the
plan file. Anything else that touches history (branching, tagging, pushing,
resetting, rewriting) is asked first.
