---
name: work
description: Executes the next unchecked step(s) of an active plan in docs/plans/ (implement, test, run the gate, commit with the plan-step suffix, tick the box). Use when the human says "work on <plan>", "next step", "continue the plan", or "do steps 3-5". Stops at the step boundary, or earlier when an open question or a missing Arris capability blocks the step.
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
3. Run the whole gate (`docs/ARCHITECTURE.md` §Gates); the hooks cover only
   part of it. A visible UI change is looked at headless (`visual-debug`).
   A failure is fixed or reported, never bypassed. The size lint is met by
   splitting the file or function; an allowlist entry needs a written
   reason and the human's OK.
4. Tick the box and commit the plan file with the step:
   `type(scope): summary (plans/<slug> step N)` (`.agents/rules/git.md`).
   One step per commit, even small ones.
5. If the step showed the plan is wrong, fix the plan in that commit (add or
   split steps, record the finding under *Open questions*) and say so.
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
