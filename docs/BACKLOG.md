# Backlog

One line per raw idea. Picking one up means `/idea` (needs thinking) or
`/plan` (obvious); the line is removed then, as is a line a roadmap cycle has
committed to. Rejected ideas keep one line below with the reason, so the same
idea is not brainstormed twice.

- Tier 1 guest SDK: `wit-bindgen`'s `export!` emits `unsafe`, which the workspace forbids; an ADR for where the exception lives, in C3 (plans/c1-m1-document step 3)
- Choice fields for plugin feature types: the world has none, so a plugin cannot declare one; an API change when M3's forms need the declaration (plans/c1-m1-document step 9)
- Structural sharing of document snapshots and an incremental DAG rebuild, each when the C1 benchmark's command-apply budget asks for it (plans/c1-m1-document steps 5 and 13)
- Upstream failure reporting: the opt-in, locally shrunk, previewed report of a kernel failure to Arris (`SEED.md` §6.6), its transport and where reports land
- Replay of public feature-history datasets through the batch API as an Arris corpus source, each dataset's licence checked first (`SEED.md` §6.6)
- Parameter-sweep continuity checks over the batch API: volume, area and topology across a swept parameter, a jump reported as a failure (`SEED.md` §6.6)
- Sketch mode's remaining ports: the sketch fillet, auto-constrain with validation's gap closing, and the constraint descriptions, when M3's sketch mode reaches them (plans/c1-m1-sketch steps 4 and 6)
- The sketch drag over its 4 ms frame budget (34 ms measured, `docs/CONCURRENCY-WASM.md` §Budgets): profile the null-space basis and retract before the budget gates, in M3's sketch-mode plan (plans/c1-m1-sketch step 3)
- A frozen feature that modified a body input: its `modifies` is unknown without its type, so it cannot be a version; none exists in M1, and a plugin that modifies bodies needs the stored slot's target recorded (plans/c1-m1-slice step 14)
- A short ADR recording that a modifying slot is a version only while its input is set, if the rule should live in ADR-0007 and not only in `docs/DATA-MODEL.md` §Bodies across features (plans/c1-m1-slice step 6)
- File with Arris (human): a hole across a fillet fails `cut` as `Internal(Split(Dangling))`, and a stadium fillet of 1 mm or less fails as `Internal(Invariant)`; recipes in `crates/arrix-kernel/tests/recipes/` (plans/c1-m2-naming steps 2, 7, 9)
- A feature giving each sweep its own index (a hole's point id), so removing one of a hole's points renames no other hole's tool; an ADR beside ADR-0011 when a scenario needs it (plans/c1-m2-features, open question 2)
- A frozen feature's `axis` input from a straight edge hashes as an `edge` input's name, since its type's form is unknown, so a plugin feature stored with an edge-sourced axis reads stale while its plugin is absent; record the input's kind in the stored result if a plugin does this (plans/c1-m2-features step 5)

## Rejected
