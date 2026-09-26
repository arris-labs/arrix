# Plan: c1-m1-sketch

- Started: 2026-09-26
- Cycle: C1, milestone M1 (`docs/ROADMAP.md` §M1), second of its three plans
- Idea: none; verbatim from the human, closing `c1-m1-document`'s OPEN 5:
  "about open 5 - yes, do it"

Runs beside `c1-m1-document` in the second plan slot. Everything up to
step 8 is independent of it; step 9 needs its evaluator (its step 9).

## Goal

`arrix-sketch` holds the ported sketcher (`SEED.md` §8.1), reviewed on the
way in and held to ArriX's rules: its ids are the document's
(`SketchEntityId`, minted by the command's author), SI inside, every
function under the size lint without an allowlist entry, the constraint
kinds limited to the ones M3's UI reaches and the rest behind feature
gates, and its test suite green on native and building for wasm. A
`core.sketch` feature stores its entities, constraints, dimensions as
expressions and last solved positions in the document, changes only by
`SketchEdit` commands with exact inverses, and evaluates by re-solving
from its stored positions. Its closed regions become keyed
`arrix_core::Profile`s whose curves carry the entity ids, so an extrude's
side faces are named by the sketch entity that swept them. A region is
referenced by its key and resolves to exactly one region or to a lost
reference, never the nearest one.

## Non-goals

- Inference (snap search, glyphs, alignment by dwell), the sketch fillet,
  auto-constrain analysis and the constraint descriptions: sketch mode's,
  ported in M3 when the UI reaches them.
- `core.extrude` and booleans: `c1-m1-slice`. The acceptance test extrudes
  a region through `arrix-kernel` directly.
- Projected geometry (an edge or vertex projected into the sketch): M2.
- Conics and splines: they stay behind the existing `conics` gate, off.
- Any UI, viewport overlay or drag gesture. Drag is ported and tested as
  solver behaviour only.

## Design deltas

- **`arrix-sketch`**: the ported model, solver (DogLeg, LM, analytic
  Jacobian, subsystems, rank analysis, drag, branch guard), diagnostics
  and validation, the planar arrangement and regions, trim, extend,
  offset and mirror. Its own sequential ids become `SketchEntityId`s, one
  id space for points, curves and constraints, supplied by the caller
  (the command's author mints them). The length tolerance it compares
  with (1 µm) moves to `arrix-core`, next to the units. `serde` joins its
  dependencies. Functions past 200 lines are split on the way in
  (diagnostics' analysis, 385; degenerate-geometry validation, 264; the
  DogLeg loop, 252).
- **Constraint kinds**: the ported sketcher's set as it shipped, less
  the conic-only kinds (behind `conics`, as they were) and `SnellsLaw`
  (behind a gate of its own, a solver stress case rather than a modelling
  need); OPEN 1, closed. Gated kinds keep their code and tests (the
  all-features run), and a document never holds them. The enabled set is
  what M3's sketch mode must reach; a kind M3 leaves without a tool is
  gated then.
- **ADR-0005**: who solves. The client solves on its thread and a
  `SketchEdit` carries the positions it produced; the authority applies
  edits without solving; the evaluator re-solves from the stored
  positions with the dimensions' expressions resolved. Stored positions
  are recipe (`docs/DATA-MODEL.md` §Sketches).
- **`arrix-doc`**: `FeatureRecord` gains an optional `sketch` (omitted
  from JSON when absent); `Command::SketchEdit { feature, edit }` with its
  inverse; dimension expressions are DAG edges to parameters; `core.sketch`
  registers as a feature type whose input is a plane (a datum, a planar
  face, a frame) or, without one, a world plane as a parameter
  (`c1-m1-document`'s OPEN 1, closed). Its output slot `sketch` carries
  the solved sketch and its regions.
- **Schema**: stays 1. The `sketch` field is additive and no document
  holding one was ever written.
- **Naming**: a region's curves are keyed by their entity ids
  (`docs/DATA-MODEL.md` §Persistent naming already says so); the key of a
  curve when one entity bounds a region in two pieces is decided in step 8
  (OPEN 3: the ported rule).

## Arris dependencies

None for the port: the sketcher is pure math and names no kernel.
Step 8's probe and the acceptance extrude use `arrix-kernel`'s `extrude`
(`ops::extrude` in the pinned `arris` 0.2.0, built in `c1-m1-document`
step 2). Ask A5 (`region2` as public API) is not needed: the ported
arrangement is used, the fallback `docs/ROADMAP.md` names.

## Steps

- [x] **[2]** Step 1 — the model: points, lines, arcs, circles with the
  construction flag; the enabled constraint kinds (OPEN 1), the rest
  gated; `SketchEntityId`s supplied by the caller; the length tolerance in
  `arrix-core`; serde as the document will write it. Ported model tests
  green, a JSON round trip, and a test that a gated kind does not
  deserialise on the default build.
- [x] **[3]** Step 2 — the solver: the system and its partition into
  independent subsystems, residuals and the analytic Jacobian, DogLeg and
  LM, the DogLeg loop split under 200 lines; and the diagnostics (DoF,
  redundancy and conflict from one rank analysis, `Diagnostics::analyze`
  split), since the ported scenarios read every verdict through them.
  The ported solver scenarios, the analytic-against-finite-difference
  gradient suite, the rank-stability, subsystem, status-split and FreeCAD
  parity tests green in SI; the same sketch solved twice gives
  bit-identical positions.
- [x] **[2]** Step 3 — drag and the branch guard: steps in the
  constraints' null space; a step that would flip an angle, a tangency
  side or an arc's sweep refused. The ported drag-policy tests. A
  200-entity drag solve measured against the 4 ms budget
  (`docs/CONCURRENCY-WASM.md` §Budgets), reported, not gated.
- [x] **[2]** Step 4 — validation: degenerate geometry and open gaps, the
  degenerate-geometry function split. Validation tests ported. (The
  diagnostics came with step 2; the prototype's `analysis_scenarios` test
  auto-constrain, which is sketch mode's, M3. Its gap closing adds
  constraints through auto-constrain's one-by-one check, so it waits for
  M3 too; the validation report counts regions, so it comes with step 5.)
- [x] **[3]** Step 5 — the arrangement and regions: pieces, loops, faces,
  `RegionKey` (bounding entity ids and an interior sample); a key resolves
  to one region or none. The same sketch twice gives the same regions in
  the same order; a key whose region was split, merged or removed resolves
  to none, never the nearest. Arrangement, region and construction-profile
  tests ported, with the validation report (its region counts) and the
  scenarios step 2 held back.
- [x] **[2]** Step 6 — trim, extend, offset, mirror, each a pure edit of
  the model with its constraint transfer. Their ported tests.
- [ ] **[2]** Step 7 — ADR-0005 (who solves), then the sketch record and
  `SketchEdit` in `arrix-doc`: add and remove entities and constraints,
  set a dimension's expression, set positions, set construction; each
  with its inverse. A dimension's expression travels on its constraint
  record (OPEN 2).
  Apply-then-inverse proptest extended to sketch edits; save, open, save
  identical with a sketch; undo restores the earlier bytes; dimension
  expressions appear as DAG edges.
- [ ] **[3]** Step 8 — regions to profiles: a region becomes an
  `arrix_core::Profile` on the sketch's plane, its curves keyed by entity
  id and, when the entity is cut, its piece number (OPEN 3).
  Probe through `arrix-kernel`: a sketched 40 × 30 mm plate with a bore
  extrudes, its side faces named by the entities that swept them; a
  region with a doubly-contributing entity extrudes with unique names.
- [ ] **[2]** Step 9 — `core.sketch` as a feature type (waits on
  `c1-m1-document` step 9): the plane from its input or its world-plane
  parameter, dimensions resolved to SI, solved from the stored positions;
  its `sketch` slot carries the solved sketch and its regions. A solve
  that does not converge, or a conflict, fails the feature soft with the
  entities named; a region key that resolves to none is `ref.lost` with
  the current regions as candidates.
- [ ] **[1]** Step 10 — the acceptance test, and the docs it proves.

## Acceptance

`cargo test -p arrix-doc --test sketched_plate` (headless, commands only):

1. Parameters `w = 40 mm`, `h = 30 mm`, `d = 10 mm`; a part; a
   `core.sketch` on world XY holding a rectangle and a circle, constrained
   and dimensioned by expressions over them, by `SketchEdit`s alone.
2. Evaluated: DoF 0, no redundancy or conflict; the holed plate is one
   region. Its profile, extruded 5 mm through `arrix-kernel`, has volume
   `(w·h − π·d²/4)·5 mm` within 1e-9 relative, and its side faces are
   named by the sketch's entity ids.
3. `w` set to 50 mm: the sketch re-solves from its stored positions, the
   same region key resolves, the volume follows.
4. The circle deleted: the holed plate's key resolves to nothing and the
   feature holding it gets `ref.lost` with candidates, never the plain
   rectangle.
5. Every command undone restores the earlier saved bytes; every command
   and its inverse round-trip through JSON.

And `cargo test -p arrix-sketch` green with the ported suite; the whole
gate green, the wasm build included; `scripts/size-allowlist.txt` still
empty.

## Docs to update on completion

- `docs/DATA-MODEL.md`: header's "Built" list; §Sketches (the record as
  built, ids, dimensions as expressions, stored positions, regions and
  their keys, ADR-0005); §The core feature types (`core.sketch`'s inputs,
  the world-plane parameter); §Commands and undo (`SketchEdit`);
  §Persistent naming (a curve key for a doubly-contributing entity).
- `docs/UI-RENDERING.md` §Sketch mode: the solver as built, the enabled
  constraint kinds, what M3 still ports (inference, sketch fillet).
- `docs/ARCHITECTURE.md`: header; §Crates (`arrix-sketch` gains `serde`;
  the tolerance in `arrix-core`); §Testing's sketch-solver row.
- `docs/CONCURRENCY-WASM.md` §Budgets: the measured drag solve.
- `docs/ROADMAP.md` status line: M1 plan 2 of 3 landed.
- `AGENTS.md` current state: the same.

## Open questions

- OPEN 1, closed 2026-09-26 by the human: the ported sketcher's
  constraint set as it shipped, less the conic-only kinds and
  `SnellsLaw`. Enabled: coincident, horizontal, vertical, horizontal and
  vertical alignment of two points, parallel, perpendicular, equal,
  concentric, tangent (line–circle, circle–circle), symmetric about a line
  and about a point, point on line, point on circle, point on a
  perpendicular bisector, midpoint, fix, block; the sketch-origin and
  axis kinds (point on, coincident with, distance to, angle with and
  symmetric across a datum; distance to the X and Y axes); dimensions:
  distance, horizontal and vertical distance, point–line, parallel-lines,
  point–circle and circle–circle distance, angle, three-point angle,
  radius, diameter, arc length.
- OPEN 2, settled by precedent: the ported sketcher carries a
  dimension's expression source on its constraint record as opaque data,
  so it travels through undo and removal, and the document resolves it
  before every solve. Ported as is unless step 7's review finds a
  problem; stated in `docs/DATA-MODEL.md` §Sketches.
- OPEN 3, settled by precedent: an entity is cut at its intersections
  into pieces numbered from 0 along it, and an uncut entity has no piece
  number, so an uncrossed sketch keys each curve by its entity id alone.
  Step 8 maps that onto `CurveKey` (a cut entity's pieces need keys of
  their own, since `Profile` refuses a key twice) and states the rule in
  §Persistent naming.
- Finding, step 2: the ported scenario suite reads its verdicts through
  `Diagnostics`, so the diagnostics moved from step 4 into step 2. The
  drag scenarios wait for step 3 and the region ones for step 5; the
  inference scenario is M3's, and a dangling-reference Jacobian case is
  gone because a sketch can no longer hold one (step 1).
- Finding, step 3: one drag frame on a 199-entity sketch (a 9 × 10 grid
  of 10 mm cells, one connected subsystem of 220 variables, its far
  corner dragged) takes 34 ms median in a release build on a Ryzen 9
  7950X, 61 ms in the dev profile: 8.5 times the 4 ms target
  (`docs/CONCURRENCY-WASM.md` §Budgets). Reported, not gated, as the step
  says. The drag reaches the UI in M3, whose sketch-mode plan has to
  profile it (the dense null-space basis and retract per follow
  iteration are the suspects) before the budget becomes a gate. The
  prototype's absolute wall-clock ceilings in its benchmarks are reported
  here instead of asserted, for the same reason; the analytic-against-FD
  ratio stays asserted.
- Finding, step 5: the prototype's `RegionKey` left hole boundaries out
  on purpose, so a hole drawn later would not lose a region. Acceptance
  step 4 needs the opposite (the holed plate's key must not resolve to
  the plain rectangle once the bore is deleted), so the key names every
  entity bounding the face, holes included. The price: a hole drawn
  inside a region later makes it another region too, and the feature
  holding the key is re-picked (`ref.lost` with candidates, step 9).
  Stated on `RegionKey`; `docs/DATA-MODEL.md` §Sketches at retirement.
  A key two faces answer to resolves to none, as the section says, and
  regions of equal area (to the tolerance squared) are ordered by their
  entities, not by the walk.
- Finding, step 6: the modify operations keep each row they add only on
  the insert-time pre-check's `Ok` (`Sketch::check_candidate`), so the
  pre-check came with them, its tests too; it tries the row on a clone
  under the lowest id the sketch does not hold, reading no minter. The
  constraint descriptions did not (M3), so the transfer table reads the
  constraint's kind itself. Break came with extend, and the hover
  previews with both, since they are pure reads of the same pieces. The
  sketch fillet (M3) and the projected-geometry cases (M2) stay out.
