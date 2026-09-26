# ArriX docs

`SEED.md` at the repository root is the charter: the problem, what ArriX is
and is not, the plugin model, the kernel's sensor, the first cycles, the
decisions taken at kickoff. The design docs describe the system as it is now;
ADRs record why it is that way.

| Doc | What it holds | State |
|---|---|---|
| [`../SEED.md`](../SEED.md) | The charter | Agreed 2026-09-26 |
| `ARCHITECTURE.md` | Crates, layer rules, the one data flow, threading, the kernel choke point and call records, errors, testing, the gate | To write (`SEED.md` §10.2) |
| `DATA-MODEL.md` | Document, DAG, features, parameters, persistent naming, frozen results, commands and undo, the `.arrx` format | To write |
| `PLUGINS.md` | The WIT world, the three tiers, manifest, contribution points, capabilities, versioning, the test kit | To write |
| `UI-RENDERING.md` | egui shell, ribbon and palette, declarative plugin UI, sketch mode, viewport, picking, visual debugging | To write |
| `CONCURRENCY-WASM.md` | Background evaluation, cancellation, the wasm build, batch evaluation, performance budgets | To write |
| `ROADMAP.md` | Cycles with goal / in / out / accept; the risk register | To write |
| [`BACKLOG.md`](BACKLOG.md) | One line per raw idea; rejected ones with the reason | Live |
| [`adr/`](adr/README.md) | Architecture decision records | Empty |
| [`ideas/`](ideas/) | Brainstorms awaiting a decision (from `ideas/TEMPLATE.md`) | Empty |
| [`plans/`](plans/) | Active plans, at most two (from `plans/TEMPLATE.md`) | Empty |

`docs/notes/` is gitignored: private working notes, never cited from a
tracked file.

Conventions: `⚠ OPEN:` marks a question deliberately left for later; the
decision that closes it gets an ADR.

## Document lifecycle

Which tier a sentence belongs to depends on how long it should stay true.

| Tier | Files | Lifetime | Rule |
|---|---|---|---|
| Charter + decisions | `SEED.md`, `adr/` | Append-only | `SEED.md` changes only by a decision that also gets an ADR. A change of mind is a new ADR superseding the old; an accepted ADR is never edited. |
| Design | `ARCHITECTURE.md`, `DATA-MODEL.md`, `PLUGINS.md`, `UI-RENDERING.md`, `CONCURRENCY-WASM.md`, `ROADMAP.md` | Living | Present tense; the system as it is *now*. The commit that changes behaviour updates the doc. Cycle progress is one status line per cycle in `ROADMAP.md`; a closed section stays under ~40 lines. |
| Ideas | `ideas/<slug>.md` | Until decided | A **brainstorm**, not a todo: problem, options with trade-offs, cost, conflicts, recommendation, the decision for the human. Accepted → absorbed by its plan and deleted; rejected → one line under "Rejected" in `BACKLOG.md`, file deleted; parked → kept with `Status: Parked`. |
| Plans | `plans/<slug>.md` | Ephemeral | Commit-sized checkbox steps, executed with a commit per step; on completion the durable parts move to the design docs and ADRs and **the plan is deleted**. At most two active. |

Raw ideas go in `BACKLOG.md`, one line each. The pipeline is walked by the
skills `/idea`, `/plan`, `/work`, `/retire-plan` and, at a cycle boundary,
`/close-cycle` (`.agents/skills/`), under the rules in `.agents/rules/`.

**Work outside the current cycle's in-list needs an ADR before it starts**,
naming the product scenario it unlocks. Scope is argued, not accreted.

**Drift review** at every cycle boundary: the agent reads each design doc
against the code, and the cycle is not closed until nothing is left
(`/close-cycle` step 2).

A finished cycle **compresses in place**: goal, one status line, in, out,
accept. There is never a second roadmap file and no `CHANGELOG.md`. What
landed lives in the git log, the status lines and the ADRs.
