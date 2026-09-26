---
name: plan
description: Write an executable plan under docs/plans/ from the template — commit-sized checkbox steps, acceptance test, design deltas, Arris dependencies, docs-to-update list — for a feature, refactor, or cycle. Use when the human says "plan", "let's do", "implement", names a cycle ("plan C1"), or accepts an idea's recommendation. Reads the idea file if one exists and absorbs it. Never starts executing.
argument-hint: <slug | cycle | accepted idea slug>
---

# /plan — from decision to todo

A plan is a todo the agent can execute step by step with a commit per step,
and the human can read in two minutes.

## Do

1. **Check the limit.** `ls docs/plans/`: two active plans (excluding
   `TEMPLATE.md`) means stop and ask which to retire first.
2. **Gather.** Read `docs/ROADMAP.md` for the cycle's in/out/accept lists,
   the design docs the work touches, the relevant ADRs, and
   `docs/ideas/<slug>.md` if the plan comes from an idea. If it does, the
   plan's *Goal* and *Design deltas* absorb the idea's decision, and the idea
   file is deleted in the same change (git keeps it; the plan header cites
   it as `Idea: docs/ideas/<slug>.md (absorbed)`). Work in no cycle's
   in-list gets, as its first step, the ADR naming the product scenario it
   unlocks.
3. **Write `docs/plans/<slug>.md`** from `docs/plans/TEMPLATE.md`:
   - *Goal* is one paragraph of what is true when done; *Non-goals* fence
     the scope.
   - *Design deltas* name each design-doc section and type that changes. A
     non-obvious decision becomes an ADR: list it as a step. A change to
     what a document persists is a schema bump with its migration and a
     fixture. A change to `arrix-plugin-api` names its semver effect.
   - *Arris dependencies* list every kernel capability the plan relies on:
     present in the pinned `arris` (cite the API) or an ask (cite the Arris
     idea or backlog line). A step on a missing one waits; it is never
     worked around.
   - *Steps* are commit-sized: each has an observable result and its own
     test or snapshot, and could be reverted alone. Order them so the
     riskiest unknown retires first; a step that depends on unprobed kernel
     behaviour starts with a probe test in the scenario's own units. Grade
     each step: **[1]** routine; **[2]** careful, a case to get right within
     a given design (a schema migration, a naming case, a plugin-API
     boundary, a snapshot whose diff has to be read); **[3]** unproven,
     behaviour to establish here (what the kernel does on an operand shape,
     a solver convergence property, what a wasm host allows).
   - *Acceptance* is an executable check, ideally the cycle's own: a test, a
     snapshot scenario or a CLI batch run. Extend the north-star gate; never
     route around it.
   - *Docs to update* is written now, while the deltas are fresh. It is what
     `/retire-plan` executes.
   - *Open questions* carry `⚠ OPEN:` items, each with who decides and by
     which step.
4. Reply with the step list and the open questions, then **stop**. The human
   edits the plan before `/work` starts it.

## Don't

- Don't execute a step. Don't create branches. Don't write code.
- Don't re-argue the idea's decision in the plan; link the ADR instead.
- Don't plan past the cycle's "out" list; put the overflow in
  `docs/BACKLOG.md`.

`$ARGUMENTS`
