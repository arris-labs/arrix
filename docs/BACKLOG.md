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
- Mirror's engine tests about a line axis: the suite is built on datum-axis fixtures, which run in the all-features build only since `MirrorAxis::Datum` went behind `unreached`, so the default build holds one engine-level line-axis test (plans/sketch-mode step 15)
- The sketch drag on a sketch of mostly nonlinear rows (a 200-line zigzag, each line's length and perpendicular to the last: 300 ms a frame): a sparse null-space basis for the tangent step, and fewer follow iterations when the cursor is out of reach (plans/sketch-mode step 1, `docs/CONCURRENCY-WASM.md` §Budgets)
- A frozen feature that modified a body input: its `modifies` is unknown without its type, so it cannot be a version; none exists in M1, and a plugin that modifies bodies needs the stored slot's target recorded (plans/c1-m1-slice step 14)
- A short ADR recording that a modifying slot is a version only while its input is set, if the rule should live in ADR-0007 and not only in `docs/DATA-MODEL.md` §Bodies across features (plans/c1-m1-slice step 6)
- File with Arris (human): a hole across a fillet fails `cut` as `Internal(Split(Dangling))`, and a stadium fillet of 1 mm or less fails as `Internal(Invariant)`; recipes in `crates/arrix-kernel/tests/recipes/` (plans/c1-m2-naming steps 2, 7, 9)
- File with Arris (human): a full-turn revolve joined to a plate it crosses, a cylinder of radius 10 mm about a world axis through the plate's top and bottom faces, fails the fuse as `kernel.naming` ("names both v11 and v12"): a circular end cap cut by a plane gets two vertices of one name, as the faces and the cap's arcs around them are alike; the `core.revolve` tests stand the cylinder on the plate, or pocket it, instead, and no recipe is saved yet (plans/c1-m2-features step 8)
- File with Arris (human): a counterbored hole's tool copied (`ops::transform`) and cut fails as `kernel.naming` ("names both e36 and e38", two edges of one name) when the copy is turned 30 or 45 degrees about a vertical axis (90 and 180 are fine), and when a translated copy is centred on the plate's edge; `core.pattern-*`'s tests use 90 degree turns and keep the holes inside the plate, and no recipe is saved yet (plans/c1-m2-features step 11)
- A feature giving each sweep its own index (a hole's point id), so removing one of a hole's points renames no other hole's tool; an ADR beside ADR-0011 when a scenario needs it (plans/c1-m2-features, open question 2)
- A frozen feature's `axis` input from a straight edge hashes as an `edge` input's name, since its type's form is unknown, so a plugin feature stored with an edge-sourced axis reads stale while its plugin is absent; record the input's kind in the stored result if a plugin does this (plans/c1-m2-features step 5)
- A plugin's own settings (for its commands, importers and exporters), in `settings.toml` under its namespace, once those contribution points exist (ADR-0015; plans/configurable-shell)
- A project config layer beside a document, and the trust model it needs before it may rebind keys or name anything that runs (ADR-0015; plans/configurable-shell)
- Config profiles: named sets of settings, keymap and theme switched together (plans/configurable-shell)
- A settings UI that writes the user's TOML through `toml_edit`, keeping their comments and order (ADR-0015; plans/configurable-shell)
- Mouse bindings in the keymap: the viewport's gestures (orbit, pan, zoom, pick) are fixed now (plans/viewport-port step 12)
- The evaluator saying "meshed" explicitly, so a generation with no final body clears the old bodies from the scene instead of leaving them until the next one with a body (plans/viewport-port step 7)
- The GPU side of the frame budget, measured on a reference GPU in CI; lavapipe says nothing about one (plans/viewport-port step 13)

## Rejected

- Everything is a plugin (the shell, the sketcher and the part features too): it reopens `SEED.md` §6.2 and §6.3 for flexibility the configurable shell mostly gives (ADR-0015, ADR-0016)
- An agent-only, headless ArriX: it gives up the interactive document `SEED.md` §3 exists for (ADR-0016)
