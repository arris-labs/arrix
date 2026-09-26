---
name: retire-plan
description: Close a completed plan — verify every step is ticked and the acceptance test passes, execute its docs-to-update list, run a drift check on the design docs it touched, update the roadmap status line and AGENTS.md current state, delete the plan file, commit. Use when the human says "retire", "close the plan", "plan is done", or after the acceptance test of a plan's last step passed and the human confirmed.
argument-hint: <plan slug>
---

# /retire-plan — move the durable parts, delete the rest

Deletion is the "done" signal. Anything worth keeping was moved first.

## Do

1. Open `docs/plans/<slug>.md`. Every step ticked? Acceptance run and green
   (run it now)? If not, stop and report which.
2. Execute *Docs to update* line by line. Design docs stay present tense:
   describe the system as it now is, with no "as of this plan" narrative. If
   an `⚠ OPEN:` was closed by a decision, write the ADR now and add it to
   `docs/adr/README.md`.
3. **Drift check** on every design doc the plan touched: read it against the
   code and fix every sentence that is no longer true. List what you fixed in
   the reply.
4. `docs/ROADMAP.md`: update the cycle's status line. If this plan completes
   a cycle, say so and suggest `/close-cycle`.
5. `AGENTS.md` "Current state": one cycle-level sentence; keep the block
   under ~15 lines.
6. Anything deferred from the plan goes to `docs/BACKLOG.md` as one line,
   naming the plan and step that deferred it. An Arris ask the plan raised
   is confirmed to exist in Arris's backlog or ideas.
7. `git rm docs/plans/<slug>.md` and commit everything as `docs: retire plan
   <slug>`, with a body listing the docs updated.
8. **Report the build cache** after the commit: `du -sh target`; if `cargo
   sweep` is installed, run `cargo sweep --time 7` and report before and
   after, otherwise leave it to the human.

## Don't

- Don't summarise the plan into a design doc: design docs hold the design,
  not the history of how it got there.
- Don't keep the plan file "for reference"; git has it.
- Don't retire with unticked boxes by editing them to ticked.

`$ARGUMENTS`
