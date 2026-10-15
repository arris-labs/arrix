---
name: work
description: Execute the next unchecked step(s) of an active plan, a commit per step, then list what is left.
disable-model-invocation: true
argument-hint: <plan slug> [step number or range]
---

# /work — one step, one commit

1. Open `docs/plans/<slug>.md`. The target is the first unchecked step, or
   the range given. Re-read its *Design deltas*, *Arris dependencies* and
   any `⚠ OPEN:` naming it. An unanswered question that is the human's, or
   a missing kernel capability, stops the step: say so.
2. Implement the step with its test or snapshot, and edit the docs it
   changes in the same step. The layer rules (`AGENTS.md`, `SEED.md` §6.7)
   are binding.
3. Run the whole gate (`docs/ARCHITECTURE.md` §Gates): `scripts/gate`, in
   the background; the pre-commit hook runs the fast half at the commit. A visible UI change is looked at headless (`visual-debug`).
   A failure is fixed or reported, never bypassed. The size lint is met by
   splitting the file or function; an allowlist entry needs a written
   reason and the human's OK.
4. Tick the box and commit the plan file with the step:
   `type(scope): summary (plans/<slug> step N)` (`.agents/rules/git.md`).
   One step per commit, even small ones.
5. If the step showed the plan is wrong, fix the plan in that commit (add or
   split steps, record the finding under *Open questions*) and say so.
   A failure that resists a first fix gets a loop before a theory: one
   command that goes red on it in seconds, shrunk until every part is
   load-bearing; then 3–5 ranked hypotheses, each with the prediction that
   would refute it, said in the reply before probing; probes one variable
   at a time, every debug print tagged `[DBG-<4 hex>]` and grepped away
   before the commit, whose body names the hypothesis that held.
6. A kernel failure becomes an Arris fixture recipe (saved from the call
   record) plus an Arris backlog line or idea, named in the reply; never an
   `#[ignore]` or a workaround here.
7. Reply with what landed, the commit, and anything surprising. Then list
   every remaining unchecked step, one line each:

   ```
   → 4  [2] the edit form: needs step 3's FormView
     5  [3] probe: fillet on a seam edge; may rewrite 6
   /work <slug> step 4  (or step 4-5)
   ```

   Mark the next one with `→`, and end with the invocation so the human can
   pick a range and a model without opening the plan. Stop unless a range
   was requested.

The last ticked box doesn't retire the plan: `/retire-plan` does, once the
human has seen the acceptance run.

`$ARGUMENTS`
