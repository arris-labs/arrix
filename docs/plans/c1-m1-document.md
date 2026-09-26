# Plan: c1-m1-document

- Started: 2026-09-26
- Cycle: C1, milestone M1 (`docs/ROADMAP.md` §M1), first of its three plans
- Idea: none; verbatim from the human: "/plan M1"

M1 is split in three plans, ordered by risk. The riskiest part, a plugin
feature on the document's one code path, comes first and needs no
sketcher:

1. **`c1-m1-document`** (this plan): the document, commands and undo, the
   evaluator and session, the kernel service for extrude, the plugin API
   0.1, the Tier 0 host and `gears.spur` live, with a downstream feature
   referencing the gear's faces by persistent name.
2. **`c1-m1-sketch`**: `arrix-sketch` ported and reviewed headless, and
   `core.sketch` with its regions as profiles. Independent of this plan;
   it can take the second plan slot at any time.
3. **`c1-m1-slice`**: `core.extrude` on sketch regions (new body, join,
   cut) and the booleans, frozen results (waits on A2), the `.arrx` zip,
   blobs and the migration mechanism, and M1's accept scenario.

## Goal

A document is built by commands alone, headless: parameters with
expressions, a datum plane, a `gears.spur` from the Tier 0 `plugins/gears`
on it, and a second datum plane offset from the gear's top face by
persistent name. Built-in and plugin features share one record shape, one
registry and one evaluator path: memoised by input hash, named from the
kernel's provenance, failing soft. Editing a parameter re-evaluates what
depends on it and every reference resolves; a reference to a tooth that no
longer exists is a lost reference with candidates. Every command has an
inverse, undo is per author, and undoing every command returns the saved
directory byte for byte to each earlier state. `LocalSession` runs the
authority and the evaluator off the calling thread, and every command and
event round-trips through JSON. `arrix eval` prints the scenario's golden
line. The plugin API 0.1's WIT world and its Rust traits are held equal by
a test. The undo granularity of plugin commands is decided by ADR.

## Non-goals

- `arrix-sketch`, `core.sketch`, `core.extrude` and booleans: plans 2 and 3.
- Frozen results, the `.arrx` zip, blobs, migrations beyond refusing a
  newer schema: plan 3 (frozen results wait on Arris A2).
- The `commands`, `exchange`, `analysis` and `ui` exports of the WIT world:
  the ADR decides plugin-command undo, and the export itself lands when a
  plugin needs it (M3's ribbon at the earliest).
- Tiers 1 and 2, the packaged test kit, anything in the app or viewport.
- Cancellation inside a kernel call (A3), parallel evaluation, tessellation.
- `arrix eval` flags (M4); `--without-plugin` arrives with plan 3.

## Design deltas

- **`arrix-core`**: `Ref`, `SlotName`, `PersistentName` (`NameRoot`,
  `NameStep`, `TopoKind`) as `docs/DATA-MODEL.md` §References and
  §Persistent naming give them, plus a canonical text encoding of a
  `PersistentName` (the WIT's `persistent-ref { encoded }`), round-trip
  tested. `Diagnostic` gains its references and ranked candidates
  (`docs/ARCHITECTURE.md` §Errors). Plain geometry for the kernel
  boundary: `Frame`, and a `Profile` of lines, arcs and circles whose every
  curve carries a caller key (a sketch entity id later, a plugin's own key
  now), which is how side faces are rooted without loop indices. `glam`
  joins here.
- **`arrix-kernel`**: the `Kernel` service, first slice: `extrude` of a
  `Profile` on a `Frame`, `face_frame`, `mass_properties`, each returning
  `KernelBody` handles and `PersistentName`s from `Provenance`; `KernelCall`
  records made before each op. Precision set for metres at the micrometre
  scale.
- **`arrix-plugin-api` 0.1.0** (API: minor, first release): the
  `wit/arrix-plugin.wit` world with `types`, `kernel` (the slice above) and
  `feature`; the Rust traits and plain types; `wit-bindgen` for the
  equality test only.
- **`arrix-doc`**: `Document`, `Part`, `FeatureRecord`, `Params` and the
  expression language, the derived DAG, the `FeatureType` registry,
  `Command` (the subset of §Commands and undo minus `SketchEdit` and
  `PluginData`), per-author undo, the evaluator and its cache, `EvalEvent`,
  `Session` and `LocalSession`, the executor module
  (`crates/arrix-doc/src/executor`, already the wasm lint's one thread
  permit), directory-form save and load. `blake3` joins here.
- **`core.datum-plane`**: offset from a planar face (`Ref::Topo`) or from
  a world plane (⚠ OPEN 1).
- **`arrix-plugin-host`**: Tier 0 hosting: a plugin's `Feature` adapted
  onto `FeatureType`; plugin version in the input hash; a panic caught and
  turned into the feature's diagnostic.
- **`arrix-cli`** depends on `arrix-gears` for the explicit Tier 0
  registration list (`docs/PLUGINS.md` §Three tiers): a new edge in the
  layer lint's allowed table, and in `docs/ARCHITECTURE.md` §Crates.
- **Schema**: stays 1. Opening a document with parts stops being refused;
  no document with parts was ever written, so no migration is owed.
- **ADR-0004**: undo granularity for plugin commands (closes the ⚠ OPEN in
  `docs/DATA-MODEL.md` and `docs/PLUGINS.md`).

## Arris dependencies

All present in the pinned `arris` 0.2.0:

- `ops::extrude(&mut Model, &Profile, Vec3, f64) -> (Body, Provenance)`
  (`arris-ops` `sweep.rs`), with `geom::Profile` of lines, arcs and circles
  and `Role::Extrude(SweepPart::…)` roots.
- `ops::query::face_frame` (`query.rs`), `ops::measure::mass_properties`
  (`measure.rs`), `Model::retain` (`arris-topo` `model.rs`).

Not needed by this plan: A2 (plan 3's frozen steps), A3 (cancellation is
between kernel calls, `docs/CONCURRENCY-WASM.md` §The evaluator), A1 (the
gear extrudes; nothing is built directly).

## Steps

- [x] **[1]** Step 1 — `arrix-core`: `Ref`, `SlotName`, `PersistentName`
  and its text encoding, `Frame`, keyed `Profile`, `Diagnostic` references
  and candidates. Proptest round trips (serde and text), grammar
  rejections.
- [x] **[3]** Step 2 — `arrix-kernel` probe and first slice: `Kernel` with
  `extrude`, `face_frame`, `mass_properties` and `KernelCall` records;
  names from provenance (caps, side faces rooted at curve keys, edges
  generated from face pairs). Probe tests in SI: a 40 × 30 mm plate with a
  bore; a 60-arc closed profile (the gear's shape class) extrudes, counts
  and volume as computed by hand; evaluating twice gives identical names
  and volumes; a zero-length extrude is a categorised error with its
  record kept.
- [x] **[3]** Step 3 — `arrix-plugin-api` 0.1.0: the WIT world (`types`,
  `kernel` slice, `feature`), the Rust traits and plain types, and a test
  holding them equal to `wit-bindgen`'s generated bindings. Builds for
  wasm32.
- [x] **[2]** Step 4 — parameters and expressions: parser over
  `arrix_core::units`, AST stored as its text, dimensional analysis at
  parse time, evaluation to SI, the functions of §Parameters. Proptest
  (print-parse round trip), unit-mismatch diagnostics.
- [x] **[2]** Step 5 — `Document`, `Part`, `FeatureRecord`, the
  `FeatureType` registry, and the DAG derived from refs and expression
  names: history-order rule, cycle refused with its nodes, dirty
  descendants in topological order. Proptest over random DAG edits.
- [x] **[2]** Step 6 — `Command` and apply: `SetParam`, `AddParam`,
  `DeleteParam`, `AddFeature`, `EditFeature`, `DeleteFeature`,
  `ReorderFeature`, `SetRollback`, `Group`; each returns the new document,
  its inverse and the touched nodes, whole or not at all. Generation
  stamps, stale rejection, a no-op leaves no undo entry, per-author undo
  and redo stacks. Property: apply then inverse is the identity.
- [x] **[2]** Step 7 — ADR-0004: undo granularity for plugin commands;
  the ⚠ OPEN in `docs/DATA-MODEL.md` and `docs/PLUGINS.md` closed.
- [x] **[2]** Step 8 — directory-form save and load: the deterministic JSON
  writer (sorted keys, two-space indent, shortest round-trip floats,
  trailing newline), `params.json` and `parts/<id>.json`; `open` reads
  parts. Save twice identical; save, load, save identical; undo of each
  command restores the earlier bytes.
- [x] **[2]** Step 9 — the evaluator, synchronous core: resolve parameters
  and inputs, the BLAKE3 input hash, the cache, fail soft (*input
  unavailable* downstream), `EvalEvent`s; `core.datum-plane`; `arrix_doc::
  eval` lines gain bodies and features. Tests: cache hit on an unchanged
  hash, a failed feature's dependents fail and the rest evaluate.
- [x] **[2]** Step 10 — Tier 0 host: `Feature` adapted onto `FeatureType`
  through the plugin API's traits only; plugin version in the hash; a
  panicking test plugin becomes a diagnostic, not a crash; the explicit
  registration list in `arrix-cli`, and the layer lint's new edge.
- [x] **[2]** Step 11 — `plugins/gears`: `gears.spur` (teeth, module,
  width, pressure angle, on a plane), involute flanks as arcs within a
  stated tolerance, checked against the true involute; evaluated twice
  from fresh state to identical names and volume; manifest
  `arrix-plugin.toml`.
- [ ] **[2]** Step 12 — naming through a plugin feature: a datum plane
  offset from the gear's top face by persistent name follows width edits
  and survives a teeth edit; a reference to tooth 20's flank after teeth
  go to 18 is a lost reference with ranked candidates, never a rebind.
- [ ] **[2]** Step 13 — `Session` and `LocalSession`: the session thread
  and the evaluator worker through the executor; replicas fed the applied
  stream with a debug hash check; events carry their generation; newest
  generation wins between features; every command and event through JSON
  and back; where a client's id seed comes from (⚠ OPEN 3).
- [ ] **[1]** Step 14 — the scenario: `tests/docs/gear-on-plane/` written
  by the acceptance test's commands and saved, and its `arrix eval` golden
  in `crates/arrix-cli/tests/golden/`.

## Acceptance

`cargo test -p arrix-doc --test gear_on_plane` (headless, through
`LocalSession` with `gears` registered):

1. Commands alone build: parameters `teeth = 20`, `m = 1 mm`,
   `w = 8 mm`; a datum plane 10 mm above world XY; `gears.spur` on it with
   its fields as expressions over those parameters; a datum plane 2 mm
   above the gear's top face, referenced by persistent name.
2. `w` edited to 12 mm: the gear and the upper plane re-evaluate, the plane
   moves by 4 mm, the lower plane is a cache hit. `teeth` edited to 24: the
   top-face reference still resolves. A reference to tooth 20's flank,
   after `teeth` goes to 18, is `ref.lost` with candidates; the features
   not depending on it evaluate.
3. The same snapshot evaluated twice: identical names and volumes.
4. Saved twice: identical bytes; saved, loaded, saved: identical bytes.
   Every command undone in reverse returns the directory byte for byte to
   each earlier save; redone, to each later one.
5. Every command and event serialised through JSON and back, equal.

And `arrix eval tests/docs/gear-on-plane` equals its golden line; the whole
gate green, the wasm build included.

## Docs to update on completion

- `docs/DATA-MODEL.md`: header's "Built" list; §Identifiers (the seed's
  source); §Parameters and expressions; §Features and §The core feature
  types (datum plane's world-plane input, or the origin feature); §References;
  §The dependency DAG; §Evaluation; §Persistent naming (the text encoding,
  curve keys); §Commands and undo (ADR-0004, the command subset built, the
  ⚠ OPEN removed); §File format "What opens today"; §Open questions.
- `docs/PLUGINS.md`: header; §One interface (the world as written, not the
  sketch); §Three tiers (Tier 0 built); §Contribution points 1 and 3
  (ADR-0004); §Versioning (0.1.0); §Open questions.
- `docs/ARCHITECTURE.md`: header's "Built" list; §Crates (`glam`,
  `blake3`, `wit-bindgen` joined; `arrix-cli` → `arrix-gears`); §The
  protocol boundary (`LocalSession` built); §The kernel choke point (the
  slice built, records without fixture export yet); §Errors (references
  and candidates).
- `docs/CONCURRENCY-WASM.md`: header; §Roles and threads; §The evaluator
  (what is built: between-call supersession, the cache); §Batch evaluation
  "Built so far".
- `docs/ROADMAP.md` status line: M1 plan 1 of 3 landed.
- `AGENTS.md` current state: the same, and the next plan.

## Open questions

- OPEN 1, closed 2026-09-26 by the human: `core.datum-plane` takes a
  world plane (XY, XZ, YZ) as a plain parameter when it has no input; no
  `core.origin` type. Step 9 builds it and writes it into
  `docs/DATA-MODEL.md` §The core feature types.
- OPEN 2, closed in step 11 by the agent: each involute flank is arcs
  within `module / 1000` of the true involute (1 µm at 1 mm, the kernel's
  point tolerance), halved at most six times (64 arcs a flank), else
  `gears.tolerance`; no bore in 0.1. Both are stated in the plugin's crate
  docs. Below the base circle a flank is a radial line; undercut is not
  modelled. Curve keys are `tooth · 1000 + k`, tooth from 1, so a teeth
  edit keeps the surviving teeth's names and loses the rest's.
- ⚠ OPEN 3: where a client's `IdMinter` seed comes from
  (`docs/DATA-MODEL.md` §Identifiers defers it to sessions). Agent
  proposes in step 13: the caller supplies it, `arrix-app` and `arrix-cli`
  read entropy at the edge, tests pass a fixed seed. An ADR only if the
  human wants one.
- OPEN 4, closed: CI's first run was green on `b72906e` (run
  36252879779, 2026-09-26), the precondition `docs/BACKLOG.md` and
  `AGENTS.md` set for M1's first plan.
- Finding, step 2: an extrude's edges and vertices are not generated from
  face pairs; Arris records each by its own sweep role, so they are roots
  (`edge:sweep.<f>.rise.<k>`). `gen` steps first appear with booleans
  (plan 3). A call-record operand is the `CallIndex` of the call that
  made the body until A2 gives bodies a content hash.
- Finding, step 3: the world differs from `docs/PLUGINS.md`'s sketch
  where the code needed it: a profile is a plain record, not a
  resource; `extrude` takes no frame (the profile carries its plane) and
  no feature id (the host knows which feature evaluates); the mass query
  is `measure`, since WIT refuses a function and a `use`d type of one
  name; `names` joins the slice so a plugin can name its own faces.
  `migrate` waits for a second `type_version`. `wit-bindgen`'s `export!`
  emits `unsafe`, which the workspace forbids, so the equality test
  implements `Guest` without exporting it; Tier 1 (C3) meets this in the
  guest SDK and needs an ADR there, not here.
- Finding, step 5: the DAG's edges are the explicit ones (refs,
  expression names, a feature's part). The implicit edges of a body
  slot's versions, a feature reading the body a join or cut before it
  made, join with join and cut in `c1-m1-slice`; nothing in this plan
  modifies a body. The DAG is rebuilt whole per command, not
  incrementally, until a budget says otherwise.
- Finding, step 6: building a document by commands alone needs parts,
  so the subset gains `AddPart` (a whole part: `DeletePart`'s inverse)
  and `DeletePart`. Stamps live in the authority, keyed by DAG node, and
  undo and redo restore them, since the file carries none.
- Finding, step 9: the world plane is a plain value, not an expression,
  so a record gains `choices` (a word the type lists; empty and omitted
  in JSON for every record so far), and `FeatureEdit` with it. The
  plugin world has no choice field: `core.datum-plane` validates its own,
  and a plugin that wants one is an API change, taken when M3's forms
  need the declaration. `FeatureType` is the plugin API's `evaluate` for
  one type, so step 10's adapter is a wrapper. A type the registry lacks
  fails as `feature.unknown-type` until plan 3's frozen results. The
  cache is bounded by entry count until body bytes (A2) give sizes, and
  the kernel keeps its call records for the evaluator's life. `EvalLine`
  gains `params` and each body line its `feature`.
- Finding, step 10: a Tier 0 plugin crate exports its manifest's text
  and its `Feature`; the host reads identity, version and `api` range
  from the manifest, so no plugin API type was needed for registration.
  `arrix-gears` joins the CLI's list here with its manifest and no types;
  step 11 gives it `gears.spur`. A plugin crate's own tests cannot reach a
  kernel (plugins depend on the API alone, dev-dependencies included), so
  the kernel-backed checks of a first-party plugin live beside the
  registration list that names it, in `arrix-cli`'s tests, until C3's
  test kit.
- OPEN 5, closed 2026-09-26 by the human: `c1-m1-sketch` starts now in
  the second plan slot, beside this one.
