# ArriX docs

`SEED.md` at the repository root is the charter: the problem, what ArriX is
and is not, the plugin model, the kernel's sensor, the first cycles, the
decisions taken at kickoff. The design docs describe the system as it is now;
ADRs record why it is that way.

| Doc | What it holds | State |
|---|---|---|
| [`../SEED.md`](../SEED.md) | The charter | Agreed 2026-09-26 |
| [`../GLOSSARY.md`](../GLOSSARY.md) | The project's words, one meaning each, and the synonyms they replace | Living |
| [`ARCHITECTURE.md`](ARCHITECTURE.md) | Crates, layer rules, the one data flow, threading, the kernel choke point and call records, errors, testing, the gate | Living |
| [`DATA-MODEL.md`](DATA-MODEL.md) | Document, DAG, features, parameters, persistent naming, frozen results, commands and undo, the `.arrx` format | Living |
| [`PLUGINS.md`](PLUGINS.md) | The WIT world, the three tiers, manifest, contribution points, capabilities, versioning, the test kit | Living |
| [`UI-RENDERING.md`](UI-RENDERING.md) | egui shell, ribbon and palette, declarative plugin UI, sketch mode, viewport, picking, visual debugging | Living |
| [`CONCURRENCY-WASM.md`](CONCURRENCY-WASM.md) | Background evaluation, cancellation, the wasm build, batch evaluation, performance budgets | Living |
| [`ROADMAP.md`](ROADMAP.md) | Cycles with goal / in / out / accept; the risk register | Living |
| [`CONFIG-CATALOG.md`](CONFIG-CATALOG.md) | Generated reference: every command, setting, theme role and feature parameter default there is to configure; a test holds it fresh (`UPDATE_SNAPSHOTS=1 cargo test -p arrix-cli catalog` regenerates it) | Generated |
| [`COMMANDS.md`](COMMANDS.md) | Which test is which accept, the off-CI golden writers, the Tier 1 build recipe | Living |
| [`../.agents/review.md`](../.agents/review.md) | What a review of a plan's diff checks, read by the reviewer at `/retire-plan` | Living |
| [`BACKLOG.md`](BACKLOG.md) | One line per raw idea; rejected ones with the reason | Live |
| [`adr/`](adr/README.md) | Architecture decision records | Append-only |
| [`ideas/`](ideas/) | Brainstorms awaiting a decision (from `ideas/TEMPLATE.md`) | Until decided |
| [`plans/`](plans/) | Active plans, at most two (from `plans/TEMPLATE.md`) | Ephemeral |

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
| Ideas | `ideas/<slug>.md` | Until decided | A **brainstorm**, not a todo: problem, options with trade-offs, cost, conflicts, recommendation, the decision for the human. Accepted → absorbed by its plan and deleted; rejected → one line under "Rejected" in `BACKLOG.md`, file deleted; parked → kept with `Status: Parked`. A plan covering part of it cuts the file down to the rest. |
| Plans | `plans/<slug>.md` | Ephemeral | Commit-sized checkbox steps, executed with a commit per step; on completion the diff is reviewed against the plan, the durable parts move to the design docs and ADRs and **the plan is deleted**. At most two active, each under ~150 lines and ~10 steps; larger work is milestone plans. |

Raw ideas go in `BACKLOG.md`, one line each. The pipeline is walked by the
skills `/idea`, `/plan`, `/work`, `/retire-plan` and, at a cycle boundary,
`/close-cycle` (`.agents/skills/`), under the rules in `.agents/rules/`.
Beside it, for the human: `/next` says where things stand and what to run,
`/handoff` writes a note a fresh session continues from, `/retro` mines a
plan's sessions for improvements to this environment, and `/wait-what`
asks for the last reply again in plain words.

**Work outside the current cycle's in-list needs an ADR before it starts**,
naming the product scenario it unlocks. Scope is argued, not accreted.

**Drift review** at every cycle boundary: the agent reads each design doc
against the code, and the cycle is not closed until nothing is left
(`/close-cycle` step 2).

A finished cycle **compresses in place**: goal, one status line, in, out,
accept. There is never a second roadmap file and no `CHANGELOG.md`. What
landed lives in the git log, the status lines and the ADRs.
