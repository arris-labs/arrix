# Plan: <slug>

- Started: YYYY-MM-DD
- Cycle: C? (or the ADR that justifies work outside the cycle)
- Idea: docs/ideas/<slug>.md (absorbed), or verbatim from the human: "…"

## Goal
One paragraph: what is true when this plan is done.

## Non-goals
What this plan deliberately does not touch, so scope cannot creep silently.

## Design deltas
Which design docs, types, crate boundaries or plugin-API items change, and
how. A non-obvious decision becomes an ADR (list it). A change to what a
document persists is a schema bump with its migration and a fixture (list
it). A change to `arrix-plugin-api` names its semver effect.

## Arris dependencies
Kernel capabilities the plan relies on, each either present in the pinned
`arris` version (cite the API) or an ask with its Arris idea or backlog line.
A missing one blocks the step that needs it; no workaround.

## Steps
Complexity: **[1]** routine — the design says what to write, the tests are
mechanical; **[2]** careful — a case to get right within a given design;
**[3]** unproven — behaviour that has to be established here.

- [ ] **[1]** Step 1 — …
- [ ] **[2]** Step 2 — …

Each step is one commit-sized unit with its own test or snapshot.

## Acceptance
The executable check (test, snapshot scenario, CLI batch run) that closes the
plan.

## Docs to update on completion
- `docs/<TOPIC>.md` §… — …
- `docs/ROADMAP.md` status line — …
- `AGENTS.md` current state — …

Written at planning time; executed before the plan is deleted.

## Open questions
`⚠ OPEN:` items, each with who decides (human / agent) and by which step.
