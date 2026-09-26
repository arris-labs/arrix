---
name: work
description: Execute the next unchecked step(s) of an active plan in docs/plans/ — implement, test, run the checks, commit with the plan-step suffix, tick the box. Use when the human says "work on <plan>", "next step", "continue the plan", or "do steps 3-5". Stops at the step boundary or when a step's open question or a missing Arris capability blocks it.
argument-hint: <plan slug> [step number or range]
---

# /work — one step, one commit

## Do

1. Open `docs/plans/<slug>.md`. The target is the first unchecked step, or
   the range given. Re-read the step's *Design deltas*, its *Arris
   dependencies* and any `⚠ OPEN:` that names it. If the open question is
   the human's and unanswered, or the kernel capability is missing, stop and
   say so. Don't guess around it.
2. Implement the step and its test or snapshot. Docs that the step changes
   are edited in the same step (`.agents/rules/git.md`). The layer rules are
   binding (`SEED.md` §6.7, `docs/ARCHITECTURE.md`): Arris only in
   `arrix-kernel`, egui only in viewport/ui/app, plugins on
   `arrix-plugin-api` alone, mutation only through commands, the kernel
   never on the UI thread, every kernel call through the choke point as a
   record.
3. Run the full gate as `docs/ARCHITECTURE.md` §Gates lists it (fmt, clippy
   with `-D warnings`, the size, wasm and layer lints, the tests, the wasm
   build). The hooks only cover part of it, and a box is ticked on a green
   suite only. For a visible UI change, look at it headless; a golden is
   refreshed only after its diff image was read.
4. Commit: `type(scope): summary (plans/<slug> step N)`; the body says why
   and cites docs and ADRs, and a plugin-API change states its semver
   effect. Tick the box in the plan and include the plan file in the same
   commit.
5. If the step showed the plan is wrong, edit the plan (add or split steps,
   record the finding under *Open questions*) in that commit and say so in
   the reply. A plan is a living todo, not a contract.
6. A kernel failure met on the way becomes an Arris fixture recipe (saved
   from the call record) and an Arris backlog line or idea, named in the
   reply; never an `#[ignore]` or workaround here.
7. Reply: what landed, the commit, the next step, anything surprising. Then
   stop unless a range was requested.

## Don't

- Don't skip ahead or fold two steps into one commit "because they're
  small".
- Don't tick a box whose test didn't run.
- Don't bypass a hook. If checks fail, fix them or stop and report.
- Don't allowlist past the size lint: split the file or function. An entry
  needs a written justification and the human's OK.
- Don't retire the plan when the last box is ticked: that is `/retire-plan`,
  which the human triggers after seeing the acceptance run.

`$ARGUMENTS`
