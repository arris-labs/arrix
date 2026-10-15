---
name: plan
description: Writes an executable plan under docs/plans/ from the template, with commit-sized graded checkbox steps, an acceptance test, design deltas, Arris dependencies and a docs-to-update list, absorbing the idea file if there is one. Use when the human says "plan", "let's do", "implement", names a cycle ("plan C1"), or accepts an idea's recommendation. Writes the plan and stops; never executes it.
argument-hint: <slug | cycle | accepted idea slug>
---

# /plan — from decision to todo

A plan is a todo the agent executes a step and a commit at a time, and the
human reads in two minutes: under ~150 lines and ~10 steps.

1. **Check the limit.** Two plans in `docs/plans/` besides `TEMPLATE.md`
   means stop and ask which to retire first.
2. **Gather** the cycle's in/out/accept from `docs/ROADMAP.md`, the design
   docs and ADRs the work touches, `GLOSSARY.md`, and `docs/ideas/<slug>.md`
   if there is one. Work in no cycle's in-list gets, as step 1, the ADR
   naming the product scenario it unlocks.
3. **Size it.** Work that will not fit ~10 steps (a whole cycle, usually)
   is split into milestone plans, `<cycle>-m1`, `<cycle>-m2`, …, each with
   an accept of its own, riskiest first. Write only the first now, and
   list the others in its *Non-goals* as one line each.
4. **Absorb the idea.** Its decision becomes the plan's *Goal* and *Design
   deltas*, and the header cites `Idea: docs/ideas/<slug>.md (absorbed)`.
   The whole idea absorbed: delete the file in the same change. Only part
   of it (the rest is a later milestone or plan): cut the file down to the
   rest and set `Status: Accepted (<option>); <part> absorbed by
   plans/<slug>`.
5. **Write `docs/plans/<slug>.md`** from `docs/plans/TEMPLATE.md`:
   - *Goal*: one paragraph of what is true when done. *Non-goals* fence
     the scope; overflow past the cycle's "out" goes to `docs/BACKLOG.md`.
   - *Design deltas*: each design-doc section and type that changes. A
     non-obvious decision is an ADR step. A change to what a document
     persists is a schema bump with its migration and a fixture. A change
     to `arrix-plugin-api` names its semver effect. A new term is named
     with its glossary entry.
   - *Arris dependencies*: every kernel capability relied on, either in the
     pinned `arris` (cite the API) or an ask (cite the Arris idea or
     backlog line). A step on a missing one waits.
   - *Steps*: commit-sized, each with an observable result and its own test
     or snapshot, revertible alone, and a few lines long; the detail a step
     needs lives in *Design deltas* or its ADR. Riskiest unknown first; a
     step on unprobed kernel behaviour starts with a probe test. Grade
     each: **[1]** routine; **[2]** careful within a given design (a schema
     migration, a naming case, a plugin-API boundary, a snapshot diff to
     read); **[3]** unproven, behaviour to establish here (kernel output on
     an operand shape, solver convergence, what a wasm host allows).
   - *Acceptance*: an executable check, ideally the cycle's own (a test, a
     snapshot scenario, a CLI batch run), extending the north-star gate.
   - *Docs to update*: written now, while the deltas are fresh;
     `/retire-plan` executes it. `GLOSSARY.md` is on it when a term is new.
   - *Open questions*: `⚠ OPEN:` items, each with who decides and by which
     step.
6. Count the lines (`wc -l`); past ~150, move step detail into the deltas
   or split as in 3.
7. Reply with the step list and the open questions, then stop: the human
   edits the plan before `/work` starts it. The plan links the idea's
   decision and leaves it settled; code and branches wait for `/work`.

`$ARGUMENTS`
