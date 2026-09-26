# Docs lifecycle: backlog → idea → plan → docs

Full rules in `docs/README.md` ("Document lifecycle"). The short version:

| Artefact | Where | Is | Lifetime |
|---|---|---|---|
| Charter | `SEED.md` | what ArriX is, the plugin model, the kickoff decisions | changed only by an ADR |
| Backlog line | `docs/BACKLOG.md` | one sentence | until picked up or rejected |
| Idea | `docs/ideas/<slug>.md` | a **brainstorm**: problem, options, trade-offs, recommendation, decision for the human. No checkboxes. | until decided: accepted → absorbed by a plan and deleted; rejected → one line under "Rejected" in the backlog with the reason, file deleted; parked → kept with `Status: Parked` |
| Plan | `docs/plans/<slug>.md` | a **todo**: commit-sized steps with checkboxes, acceptance test, docs-to-update list | until retired: durable parts moved to design docs and ADRs, then deleted |
| Design doc | `docs/<TOPIC>.md`: `ARCHITECTURE`, `DATA-MODEL`, `PLUGINS`, `UI-RENDERING`, `CONCURRENCY-WASM`, `ROADMAP` | the system as it is now, present tense | living |
| ADR | `docs/adr/` | a decision and its reasons | append-only |
| Notes | `docs/notes/` (gitignored) | private pondering | until it becomes an idea, an ADR or nothing |

- Not every backlog line becomes an idea, and not every idea becomes a plan.
  A small, obvious task goes straight to a plan; a large or contested one
  gets an idea first.
- At most **two active plans**. A third means retire one first. A plan
  lives in `docs/plans/`, never at the repository root.
- An idea never contains implementation steps; a plan never re-argues the
  decision its idea already made. It links the ADR if one was needed.
- **Work outside the current cycle's in-list needs an ADR naming the product
  scenario it unlocks *before* its plan starts.** The idea proposes that
  ADR; the plan's first step writes it.
- **Core or plugin** is asked of every new capability (`SEED.md` §6.2): an
  idea says which and why; a plan that puts a domain into the core needs an
  ADR.
- A kernel capability ArriX lacks is not planned around. It goes to Arris as
  a fixture, a backlog line or an idea there, and the plan names it under
  *Arris dependencies*.
- Skills: `/idea`, `/plan`, `/work`, `/retire-plan` walk this pipeline;
  `/close-cycle` closes a cycle once its last plan is retired: drift review,
  the finished section compressed to its status line, the next section
  confirmed. One roadmap file, always.
- A **closed roadmap section is five parts and under ~40 lines**: goal, one
  status line (the risk retired, the ADRs taken), in, out, accept. A fact
  that lives only in the narrative is moved to the topic doc or the ADR
  that owns it, never dropped.
- **`AGENTS.md` "Current state" stays under ~15 lines**, at cycle
  granularity. Progress narrative belongs in the roadmap's status line and
  the plan being executed.
- **No `CHANGELOG.md`.** What landed is in the git log (a commit per plan
  step), the roadmap's status lines and the ADRs.
