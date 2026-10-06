# Plan: c1-m2-features

- Started: 2026-10-06
- Cycle: C1, M2 (the second of its two plans; `c1-m2-naming` landed the
  kernel half)
- Idea: none. Verbatim from the roadmap: "the part features and projection
  (`c1-m2-features`) next" (`docs/ROADMAP.md` §C1 status, §M2)

## Goal

Every core feature type of `docs/DATA-MODEL.md` §The core feature types is
built in the document: `core.revolve`, `core.fillet`, `core.chamfer`,
`core.hole` (simple, counterbore, countersink; blind or through),
`core.pattern-linear`, `core.pattern-circular`, `core.mirror`, and
`core.boolean` in full (`fuse`, `cut`, `common`; several tools). Each goes
through the plugin API's `Kernel`, as a built-in must, so plugin API 0.4
offers the same operations to plugins. A sketch can hold an edge or vertex
of a body projected onto its plane, re-projected on every evaluation and
lost like any reference. M2's accept holds at the document level: the
naming scenarios, each an edit by command followed by a check that a
downstream `Ref::Topo` resolves to the same entity, their opposite cases
ending in `ref.lost` with candidates while independent features still
evaluate, and an oversize `core.fillet` failing as
`kernel.degenerate.blend-too-large` with `eval --save-fixtures` writing its
recipe.

## Non-goals

- Forms, ribbon entries, picking, previews: M3. A feature here is made by
  command and tested headless; `param-spec`s and titles are written, not
  rendered.
- Choice fields in the plugin API (`docs/BACKLOG.md`): core types keep
  reading `choices` undeclared, as `core.extrude` does.
- `cut_many`/`fuse_many` (unreleased in Arris); several tools are
  sequential calls.
- A fix for a split piece's order on a redraw (the Arris ask in
  `docs/ROADMAP.md` §C1 Arris dependencies) or for the hole across a
  fillet (`docs/BACKLOG.md`, the human files it): the scenarios avoid
  both, and say so.
- Ellipses or splines from projection: the `conics` gate stays off; a
  projection that is neither a line, an arc nor a circle fails soft.
- The cycle's bracket accept, STEP/STL export, `--sweep`: M4.
- Sweep, loft, shell, draft (`docs/ROADMAP.md` §C1 out).

## Design deltas

- **ADR-0012, inputs that are lists and the new input kinds** (step 1).
  A record's input holds one `Ref` or a list of them (`FeatureRecord.inputs:
  BTreeMap<String, Input>`, `Input` serialised untagged as a `Ref` object
  or an array, so every schema-2 record reads unchanged). An `input-spec`
  says whether it takes a list. New kinds: `edge` (a `Ref::Topo` edge,
  checked against the feature's `target` body and passed as its name),
  `point` (a sketch point or a vertex, as a world point), `axis` (a sketch
  line, a straight edge, or a world axis by choice, as a point and a
  direction) and `tool` (a feature's kept tool, per ADR-0013). Product
  scenario: a fillet of four edges, a hole at three sketch points.
- **ADR-0013, what a pattern or a mirror copies** (step 1, decided under
  the open questions): a body, or the tool of a feature that joins or
  cuts. A feature that joins or cuts keeps its tool as a
  hidden output the evaluator holds, readable only by `tool` inputs
  (not a part body, never `slot.consumed`), and a pattern of features
  copies their tools, fuses the copies and applies them with each source's
  own mode: the `c1-m2-naming` shape (`inst` steps, raising the count
  renames no copy that stays).
- **Schema 3** (step 4): list inputs and a sketch entity's projection
  source. Migration 2 → 3 is the identity bar the header, with
  `tests/docs/migrations/schema-2/` as its fixture (a copy of
  `tests/docs/slice/`, re-saved to the slice's bytes but the header). An
  older build refuses a schema-3 document by the existing rule.
- `arrix-core`: `Input` (one or many `Ref`s); `FeatureEdit` sets a list
  input whole (its inverse the old value).
- `arrix-sketch`: an entity may carry `source: Ref` (a projected edge or
  vertex); the solver treats a sourced entity's points as fixed. Lines,
  arcs, circles and points only.
- `arrix-kernel`: `common`; `project` (edges and vertices of a body by
  name onto a frame, through `ops::query::project_to_plane`, recorded,
  `kernel.degenerate.projection-collapses` and
  `kernel.unsupported.projection` for a curve the sketch has no entity
  for); naming of several booleans in one feature (step 2's probe decides
  whether a call number is needed in the `mod`/`gen` steps; if so it is an
  ADR, not a patch).
- `arrix-doc`: `core_revolve.rs`, `core_blend.rs` (fillet and chamfer),
  `core_hole.rs`, `core_pattern.rs` (linear, circular), `core_mirror.rs`;
  `core_boolean.rs` extended; the evaluator resolves the new kinds and a
  sketch's projected entities before the solve (like dimensions, so
  `core.sketch` needs no kernel call of its own) and keeps join/cut tools
  per ADR-0013.
- **`arrix-plugin-api` 0.4** (step 5). API: minor (breaking under 0.x):
  `Kernel` gains `revolve`, `fillet`, `chamfer`, `common`, `copy`,
  `mirror`; `input-kind` gains `edge`, `point`, `axis`, `tool`;
  `input-spec` gains `list`; `input-value` gains the matching values. The
  WIT world and the Rust traits change together under `wit_equality.rs`;
  `gears` takes 0.4 with no code change beyond its `api` range.
- `core.hole`'s tool: one revolved profile per point, its curves keyed by
  the point's entity id mixed with the segment number, the points taken in
  id order, so moving a point renames nothing and removing one renames the
  later holes' roots (decided under the open questions).

## Arris dependencies

All in `arris` 0.5.0, already pinned; no ask is needed.

- `ops::revolve`, `ops::fillet`, `ops::chamfer`, `ops::transform`,
  `ops::mirror`, `fuse`, `cut`: built in `arrix-kernel` by `c1-m2-naming`.
- `ops::common` (`arris_ops::boolean::common`): present, not yet wrapped.
- `ops::query::project_to_plane`: present, not yet wrapped. Returns
  `Projection::Vertex` and `Projection::Edge` with a `Curve2` (a line, a
  circle or an ellipse); `Reason::ProjectionCollapses` for an edge seen
  edge-on.
- Known gaps the scenarios steer clear of, not worked around: a hole across
  a fillet (`Internal(Split(Dangling))`), a stadium fillet of 1 mm or less
  (`Internal(Invariant)`), a split piece's order on a redraw (the
  `ConsumerKey` ask). Each is in `docs/BACKLOG.md` or the roadmap's
  dependency table.

## Steps

- [x] **[2]** Step 1 — ADR-0012 (list inputs, the `edge`, `point`, `axis`
  and `tool` kinds) and ADR-0013 (a pattern or a mirror of features
  copies their tools). Docs only: `docs/DATA-MODEL.md` §Features and
  §References say where the ADRs land; nothing is built.
  *Done: the `feature` kind is named `tool` (it resolves to the tool, not
  the feature); an `edge` is checked against its body by the kernel call
  (`kernel.unknown-name`), not by the resolver; a frozen source has no
  tool (`input.no-tool`); a type returns its tool as `tool-output` beside
  its slots.*
- [x] **[3]** Step 2 — probe tests in `arrix-kernel/tests/probe_m2_features.rs`
  (metres): `common` of a plate and a box (what each piece is named); two
  `cut`s and a `fuse` in one feature (do any two entities share a `gen` or
  `mod` name across the calls?); a counterbored hole tool revolved and
  cut, then the same with the point moved; `project_to_plane` of a
  plate's top edges, a hole's rim and a vertex onto the top face and onto
  a tilted plane. Then `Kernel::common` and `Kernel::project`, recorded,
  with their fixture-recipe steps. If calls in one feature clash, stop:
  the naming fix is an ADR the human reviews (now step 3).
  *Done: they clash. A second cut of one feature splits piece `0` of the
  top face again and names its piece `1` `mod.<F>.1` a second time
  (`kernel.naming`); under two features, or with the tools fused first
  and cut once (disjoint or crossing), the names are clean. `common`
  names as `fuse` does; the counterbored tool moved keeps every name;
  `project` returns `Projected` (a point, a line, an anticlockwise arc in
  the plane's `(u, v)`, a circle), end-on edges and revolve seams
  `projection-collapses`, ellipses `kernel.unsupported.projection`.
  Step 3 is new, for the human's decision under the open questions.*
- [x] **[2]** Step 3 — ADR-0014, several booleans in one feature, as the
  human decides it under the open questions; then built: as recommended,
  `fuse`, `cut`, `common`, `fillet` and `chamfer` take the feature's
  boolean number `n`, written `mod.<F>.<n>.<k>` and `gen.<F>.<n>[…]` with
  `n` 0 unwritten (as ADR-0011's sweep index), the name grammar in
  `arrix-core` and its round-trip tests, `probe_m2_features.rs`'s clash
  test turned to the clean case, `docs/DATA-MODEL.md` §Persistent naming
  and the ⚠ OPEN in `docs/ARCHITECTURE.md` §The kernel choke point
  resolved. Blocks steps 5 (the API carries `n`), 7, 11 and 12.
  *Done: ADR-0014; `NameStep::{Modified, Generated}` carry `boolean`;
  `Kernel::{fuse, cut, common, fillet, chamfer}` take it after the
  feature and the records hold it (`serde(default)`); the host's
  `FeatureKernel` counts a feature's booleans and blends together. The
  plugin API is unchanged here: step 5 decides whether a plugin sees
  `n` (default: it does not, as for sweeps).*
- [x] **[2]** Step 4 — schema 3 in `arrix-core` and `arrix-doc`: `Input`,
  the list form in JSON, `FeatureEdit` of a list input with its inverse,
  a sketch entity's `source`, the 2 → 3 link and its fixture, every save
  and undo byte test still green.
  *Done: `Input` in `arrix-core`; `FeatureRecord.inputs` and
  `FeatureEdit.inputs` hold it; the DAG reads one edge per element and
  per sketch source; `Sketch.sources` (a map by entity id, `Ref::Topo`
  only, on a point, line, arc or circle; `SketchEdit.sources`; removal
  sweeps) is stored and diffed but nothing reads it for geometry until
  step 6. The evaluator refuses any list as `input.list` until step 5
  gives `input-spec` its `list`; a frozen feature's input hash takes a
  list's elements in order. `SCHEMA` 3, the 2 → 3 link,
  `tests/docs/migrations/schema-2/`, every scenario's header re-saved.*
- [x] **[2]** Step 5 — plugin API 0.4: the WIT world, the Rust traits, the
  equality test; the evaluator's host implements the new `Kernel`
  methods over `arrix-kernel` and resolves the new input kinds (a list
  input as its elements in order; an `edge` not on the body is the
  kernel's `kernel.unknown-name`; a lost name `ref.lost` with candidates
  as today; `feature-output.tool` held by the evaluator, ADR-0013).
  `gears` on 0.4; `docs/PLUGINS.md` §One interface and §Versioning in the
  same commit.
  *Done: API 0.4.0 as ADR-0012/0013 say, with one `tool` record for both
  the input and the output (not a separate `tool-output`), `list` written
  `%list` in WIT, `Axis` and `Motion` in `arrix-core` (`Motion` moved from
  the kernel, re-exported there); `copy` takes its instance index and the
  rest of the numbering is the host's (ADR-0014: a plugin does not see
  `n`). `arrix-kernel` gained recorded `vertex_point` and `edge_axis` for
  the `point` and `axis` inputs from the body. The evaluator holds a
  kept tool in its cache entry and state, hashes the new kinds by value
  (a tool by its source's hash), refuses a list on a single input and a
  lone reference on a list input as `input.list`, and `core.extrude`
  keeps its tool in join and cut. Tests: `eval/input_tests.rs`,
  `arrix-kernel/tests/locate.rs`, the equality test.*
- [x] **[3]** Step 6 — projection: the evaluator projects each sourced
  sketch entity before the solve, `arrix-sketch` holds it fixed, a lost
  source fails the sketch as `ref.lost` naming the entity, an edge-on or
  elliptic projection as its `kernel.*` code. Tests: a sketch on a
  plate's top face projecting the top edges stays on its branch after a
  width edit; a constraint to a projected line drives a drawn one; the
  projected edge removed upstream gives `ref.lost`. Starts by checking
  whether `arrix-sketch`'s datum handling already fixes points, and stops
  for the human if fixing them changes the solver's rank or the drag
  policy.
  *Done: no stop needed. A fixed point is already not a solver variable
  and a pin on it is dropped, so a projected entity's points join that
  set (`Sketch::held_points`) and a projected circle's radius gets no
  column: the same mechanism, no new rank or drag rule. The evaluator
  projects in `resolve` after the inputs (`eval/project.rs`), one
  recorded call per entity, so the input hash reads the projected
  positions; a line's ends are matched to the nearer projected ends. A
  projection of another shape than the entity is `sketch.source-kind`.
  Tests: `arrix-sketch/tests/projected.rs`,
  `arrix-doc/tests/projected_sketch.rs` (width edit, a drawn line driven,
  the edge redrawn away upstream as `ref.lost`, an end-on edge, a shape
  mismatch).*
- [x] **[1]** Step 7 — `core.boolean` in full: `op` `common`; `tools` a
  list body input (each consumed), several tools applied one call each.
  `core.boolean`'s `type_version` becomes 2, migrated from 1's single
  `tool` (third open question). Tests beside
  `crates/arrix-doc/tests/core_boolean.rs`.
  *Done: the migration is a type-level link run after the schema chain on
  every open (`migrate::core_types`), not a schema bump, so no document
  needs a new schema; `tests/docs/slice` was re-saved at version 2 (the
  diff is the one record) and `migrations/schema-2` stays at version 1 as
  its fixture. `bodies::effects` now reads every element of a list body
  input, so each tool is consumed.*
- [x] **[2]** Step 8 — `core.revolve`: a `region`, an `axis`, an `angle`
  (default a full turn), `mode` and `target` as `core.extrude` has them,
  the same failure codes under `revolve.*`. Tests: a full and a partial
  turn, join and cut, an axis through the region (`kernel.*`, record
  kept).
  *Done: `place` in `core_extrude.rs` is the shared new/join/cut step of
  both types, so the codes differ only by prefix. A full-turn cylinder
  fused across a plate fails `kernel.naming` (two vertices where a plane
  cuts the end cap's circle name alike); the tests stand it on the plate
  or pocket it, and the gap is a backlog line for Arris, no recipe saved
  yet.*
- [x] **[2]** Step 9 — `core.fillet` and `core.chamfer` (`core_blend.rs`):
  a `target` body and a list of `edges`; `radius`, or `distance`; the
  slot modifies `target`. Tests: four edges of a plate; a chain named by
  one edge; a blend of a blended edge; an oversize fillet failing as
  `kernel.degenerate.blend-too-large` with its record. `tests/docs/oversize-fillet/`
  replaces `zero-plate` in `crates/arrix-cli/tests/save_fixtures.rs`, and
  its recipe golden parses under the vendored grammar.
  *Done: one `CoreBlend` type for both. The oversize document is what
  `save_fixtures.rs` builds by commands (`scenario_directory`), and its
  recipe is held equal, bar the step numbering (call 1 is the edge's
  locate, no step), to the kernel's own `oversize-fillet` golden, which
  the vendored grammar reads. An edge not on the body is `ref.lost` with
  candidates from the resolver, as every name. `zero-plate` is gone.*
- [x] **[2]** Step 10 — `core.hole`: a `target`, a list of `points` (sketch
  points), `diameter`, `extent` choice `blind` (with `depth`) or
  `through`, `kind` choice `simple`, `counterbore` (`cb-diameter`,
  `cb-depth`) or `countersink` (`cs-diameter`, `cs-angle`); one revolved
  tool per point along the sketch plane's inward normal, fused, cut.
  Tests: each kind and extent; volumes against the analytic value; a point
  moved keeps every hole's wall and rim names; a point off the body cuts
  nothing and says so (`hole.misses`).
  *Done: not as the design delta had it. A `point` input carries no entity
  id, so the profile's curve keys are the section's segment numbers and the
  hole's place in the list is its sweep index; and the tools are cut one by
  one, not fused and cut once, because a single cut's boolean number would
  be 0 for one hole and 1 for two (the fuse takes 0), renaming the first
  hole's faces when a second is added. The fused tools are kept after the
  cuts for step 11. A through hole's tool is 1 m long: the kernel offers no
  extent of a body.*
- [x] **[2]** Step 11 — `core.pattern-linear` and `core.pattern-circular`
  over a body or over features, per ADR-0013: `count`, `spacing` along a
  `direction` (an `axis`), or `count` and `angle` about an `axis`; copies
  through `Kernel::copy`, fused, applied. Tests: a hole patterned 3, then
  5, then 2: the surviving holes' names unchanged; a pattern of a body
  joined; a copy that misses the target says so.
  *Done: one `CorePattern` type for both, with `apply` in `core_pattern.rs`
  for step 12's mirror. The ADR-0013 shape changes once more, for the
  reason step 10 found: a pattern's copies are applied one by one, not
  fused into one tool, since the cut's boolean number would be the
  count's and raising the count would rename every surviving copy. A body
  pattern is the `target`'s own copies fused into it (no `tools`), not a
  separate `body` input. Two gaps met and steered around, for the human
  to file with Arris: a counterbored hole's copy turned by 30 or 45
  degrees fails `cut` as `kernel.naming` (90 and 180 are fine), and a
  counterbored hole's copy centred on the plate's edge fails the same way.*
- [ ] **[1]** Step 12 — `core.mirror` over a body or features about a
  `plane` input, through `Kernel::mirror` and ADR-0013's path. Tests: a
  boss mirrored and joined; the mirror plane moved.
- [ ] **[2]** Step 13 — the document-level naming scenarios,
  `crates/arrix-cli/tests/naming_scenarios.rs`, over one document saved to
  `tests/docs/naming/` (a plate, a pocket, a counterbored hole patterned
  linearly, a boss, a fillet on a top edge, a sketch projecting the
  plate's top edges): a width edit under the fillet, the hole moved
  across the plate (clear of the fillet), the pattern's count raised and
  lowered, a sketch curve redrawn between two kept ones, a feature
  inserted before the filleted edge; each by command, each checked by the
  downstream `Ref::Topo` resolving and the face it names keeping its
  name. The opposite cases (the referenced edge removed) end in `ref.lost`
  with candidates, and the features that do not read it evaluate. Undo of
  every command returns each earlier save's bytes.

Each step is one commit. Steps 2 and 6 begin as probes and may rewrite the
steps after them.

## Acceptance

`cargo test -p arrix-cli --test naming_scenarios` and `cargo test -p
arrix-cli --test save_fixtures` (on `tests/docs/oversize-fillet/`), with
the whole gate green, the wasm build included. Together they are M2's
accept in `docs/ROADMAP.md`: the five naming scenarios and their opposite
cases at the document level, and the oversize fillet's
`kernel.degenerate.blend-too-large` with a recipe that parses as an Arris
fixture. That the recipe fails the same way in Arris's own runner is
checked once by the human against the local Arris checkout, as for
`c1-m2-naming`.

## Docs to update on completion

- `docs/DATA-MODEL.md` §The core feature types — each type as built
  (inputs, choices, defaults, failure codes), as `core.extrude`'s
  paragraph is; the table's `core.mirror` row (built) and `core.boolean`'s
  "M1" note gone.
- `docs/DATA-MODEL.md` §Features and §References — list inputs, the new
  kinds (ADR-0012); §Bodies across features — the tool kept for patterns
  (ADR-0013).
- `docs/DATA-MODEL.md` §Sketches — projected entities as built; §File
  format — schema 3 and its link; §Persistent naming — several booleans
  in one feature, the hole's keys, the pattern of features.
- `docs/ARCHITECTURE.md` §The kernel choke point — `common` and `project`
  built; the operations plugins now get.
- `docs/PLUGINS.md` §One interface, §Versioning — 0.4.
- `docs/ROADMAP.md` — M2's status line (landed), the risk row "Names do
  not hold" (document half retired by `naming_scenarios.rs`).
- `docs/BACKLOG.md` — any overflow found on the way.
- `AGENTS.md` current state — M2 landed, M3 next.

Written at planning time; executed before the plan is deleted.

## Open questions

- Decided 2026-10-06 (the human chose (a)): **a pattern or a mirror of
  features copies the tools of features that join or cut**, kept by the
  evaluator as hidden outputs; ADR-0013 records it in step 1. Not chosen:
  (b) bodies only, a patterned hole being three features where a user
  expects one; (c) re-evaluating the source feature's type at each copy's
  frame, which `evaluate` has no frame argument for and which costs `N`
  times the kernel calls.
- Decided 2026-10-06 (the human accepted the recommendation): **removing
  a hole's point renames the later holes' tools.** Points are taken in id
  order and the sweep index is call order (ADR-0011), so a removal shifts
  the `revolve.<F>.<n>` root of every later hole; moving a point renames
  nothing. Step 10 states this in `docs/DATA-MODEL.md` §Persistent naming;
  a feature giving a sweep its own index is a backlog line, an ADR when a
  scenario needs it.
- Decided 2026-10-06 (the human chose (a)): **a boolean number in the
  step**, ADR-0014. The question as asked: **how a feature names several
  booleans on one target.** Step 2 found that the second of two cuts by one
  feature splits piece `0` of what the first split and names its piece
  `1` as the first did, so the result is refused as `kernel.naming`.
  It concerns `core.boolean` with several tools (step 7: `common` cannot
  fuse its tools first), a pattern or mirror of features whose sources
  join and cut (steps 11 and 12, each source applied with its own mode),
  and any tool-side combination of three or more overlapping tools.
  `core.hole` (fuse the tools, cut once) is clean either way.
  (a) *Recommended:* a boolean number in the step, `mod.<F>.<n>.<k>` and
  `gen.<F>.<n>[…]`, `n` the feature's boolean in call order, 0 not
  written. It is ADR-0011's answer for sweeps, every existing name and
  document reads unchanged, and any feature may make any number of
  booleans. Its cost is a grammar change in `arrix-core`, an `index`
  argument on five `Kernel` methods and on the 0.4 API, and that a tool
  inserted earlier renames the later booleans' pieces. That is the trade
  already accepted for a hole's points.
  (b) one boolean per target per feature: the tools are combined first
  (fused for `fuse` and `cut`, intersected for `common`), and a pattern
  of features is refused when its sources' modes differ. No grammar
  change, but it restricts ADR-0013, and combining three overlapping
  tools can still split one entity twice.
  (c) the split number `k` continued across a feature's calls: rejected
  in the probe's reading, since `k` would then depend on how many pieces
  earlier calls made, and an upstream edit would rename.
- Decided at step 7 (the agent, the default): `core.boolean` becomes
  `type_version` 2 with a migration on open, not both shapes forever.
