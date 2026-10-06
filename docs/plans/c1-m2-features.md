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
  direction) and `feature` (for patterns and mirror, per ADR-0013). Product
  scenario: a fillet of four edges, a hole at three sketch points.
- **ADR-0013, what a pattern or a mirror copies** (step 1, decided under
  the open questions): a body, or the tool of a feature that joins or
  cuts. A feature that joins or cuts keeps its tool as a
  hidden output the evaluator holds, readable only by `feature` inputs
  (not a part body, never `slot.consumed`), and a pattern of features
  copies their tools, fuses the copies and applies them with each source's
  own mode: the `c1-m2-naming` shape (`inst` steps, raising the count
  renames no copy that stays).
- **Schema 3** (step 3): list inputs and a sketch entity's projection
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
- **`arrix-plugin-api` 0.4** (step 4). API: minor (breaking under 0.x):
  `Kernel` gains `revolve`, `fillet`, `chamfer`, `common`, `copy`,
  `mirror`; `input-kind` gains `edge`, `point`, `axis`, `feature`;
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

- [ ] **[2]** Step 1 — ADR-0012 (list inputs, the `edge`, `point`, `axis`
  and `feature` kinds) and ADR-0013 (a pattern or a mirror of features
  copies their tools). Docs only: `docs/DATA-MODEL.md` §Features and
  §References say where the ADRs land; nothing is built.
- [ ] **[3]** Step 2 — probe tests in `arrix-kernel/tests/probe_m2_features.rs`
  (metres): `common` of a plate and a box (what each piece is named); two
  `cut`s and a `fuse` in one feature (do any two entities share a `gen` or
  `mod` name across the calls?); a counterbored hole tool revolved and
  cut, then the same with the point moved; `project_to_plane` of a
  plate's top edges, a hole's rim and a vertex onto the top face and onto
  a tilted plane. Then `Kernel::common` and `Kernel::project`, recorded,
  with their fixture-recipe steps. If calls in one feature clash, stop:
  the naming fix is an ADR the human reviews before step 5.
- [ ] **[2]** Step 3 — schema 3 in `arrix-core` and `arrix-doc`: `Input`,
  the list form in JSON, `FeatureEdit` of a list input with its inverse,
  a sketch entity's `source`, the 2 → 3 link and its fixture, every save
  and undo byte test still green.
- [ ] **[2]** Step 4 — plugin API 0.4: the WIT world, the Rust traits, the
  equality test; the evaluator's host implements the new `Kernel`
  methods over `arrix-kernel` and resolves the new input kinds (a list
  input in the form's order; an `edge` not on the `target` body is
  `input.wrong-body`; a lost name `ref.lost` with candidates as today).
  `gears` on 0.4; `docs/PLUGINS.md` §One interface and §Versioning in the
  same commit.
- [ ] **[3]** Step 5 — projection: the evaluator projects each sourced
  sketch entity before the solve, `arrix-sketch` holds it fixed, a lost
  source fails the sketch as `ref.lost` naming the entity, an edge-on or
  elliptic projection as its `kernel.*` code. Tests: a sketch on a
  plate's top face projecting the top edges stays on its branch after a
  width edit; a constraint to a projected line drives a drawn one; the
  projected edge removed upstream gives `ref.lost`. Starts by checking
  whether `arrix-sketch`'s datum handling already fixes points, and stops
  for the human if fixing them changes the solver's rank or the drag
  policy.
- [ ] **[1]** Step 6 — `core.boolean` in full: `op` `common`; `tools` a
  list body input (each consumed), several tools applied one call each.
  `core.boolean`'s `type_version` becomes 2, migrated from 1's single
  `tool` (third open question). Tests beside
  `crates/arrix-doc/tests/core_boolean.rs`.
- [ ] **[2]** Step 7 — `core.revolve`: a `region`, an `axis`, an `angle`
  (default a full turn), `mode` and `target` as `core.extrude` has them,
  the same failure codes under `revolve.*`. Tests: a full and a partial
  turn, join and cut, an axis through the region (`kernel.*`, record
  kept).
- [ ] **[2]** Step 8 — `core.fillet` and `core.chamfer` (`core_blend.rs`):
  a `target` body and a list of `edges`; `radius`, or `distance`; the
  slot modifies `target`. Tests: four edges of a plate; a chain named by
  one edge; a blend of a blended edge; an oversize fillet failing as
  `kernel.degenerate.blend-too-large` with its record. `tests/docs/oversize-fillet/`
  replaces `zero-plate` in `crates/arrix-cli/tests/save_fixtures.rs`, and
  its recipe golden parses under the vendored grammar.
- [ ] **[2]** Step 9 — `core.hole`: a `target`, a list of `points` (sketch
  points), `diameter`, `extent` choice `blind` (with `depth`) or
  `through`, `kind` choice `simple`, `counterbore` (`cb-diameter`,
  `cb-depth`) or `countersink` (`cs-diameter`, `cs-angle`); one revolved
  tool per point along the sketch plane's inward normal, fused, cut.
  Tests: each kind and extent; volumes against the analytic value; a point
  moved keeps every hole's wall and rim names; a point off the body cuts
  nothing and says so (`hole.misses`).
- [ ] **[2]** Step 10 — `core.pattern-linear` and `core.pattern-circular`
  over a body or over features, per ADR-0013: `count`, `spacing` along a
  `direction` (an `axis`), or `count` and `angle` about an `axis`; copies
  through `Kernel::copy`, fused, applied. Tests: a hole patterned 3, then
  5, then 2: the surviving holes' names unchanged; a pattern of a body
  joined; a copy that misses the target says so.
- [ ] **[1]** Step 11 — `core.mirror` over a body or features about a
  `plane` input, through `Kernel::mirror` and ADR-0013's path. Tests: a
  boss mirrored and joined; the mirror plane moved.
- [ ] **[2]** Step 12 — the document-level naming scenarios,
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

Each step is one commit. Steps 2 and 5 begin as probes and may rewrite the
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
  nothing. Step 9 states this in `docs/DATA-MODEL.md` §Persistent naming;
  a feature giving a sweep its own index is a backlog line, an ADR when a
  scenario needs it.
- ⚠ OPEN: whether the M1 single-`tool` form of `core.boolean` becomes
  `type_version` 2 with a migration (planned, step 6) or reads both
  shapes forever. Agent, at step 6; a type migration is the default.
