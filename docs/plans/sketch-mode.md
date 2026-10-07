# Plan: sketch-mode

- Started: 2026-10-07
- Cycle: C1, M3 (in-list: "Sketch mode: drawing, inference that commits
  what it shows, dimensions as expressions, trim/extend/offset/mirror,
  regions shaded as extrude will take them"; "one gesture = one command,
  checked")
- Idea: none; verbatim from the human: "next thing in m3", sketch mode chosen
  over the forms.

## Goal

A user creates a sketch on a world plane, a planar face or a datum plane,
or opens an existing one from the tree; the camera turns to face it in
orthographic projection; they draw lines, rectangles, circles and arcs
with inference that commits exactly the constraints it drew, drag
geometry with the solver following the cursor within the frame budget,
add constraints and dimensions whose values are expressions, trim, extend,
offset and mirror, see the closed regions shaded as extrude will take them,
undo and redo any of it, and finish. Every gesture is exactly one command,
or none when nothing changed, and a scenario counts them. The sketch-drag
solve on a 200-entity sketch is inside its 4 ms target and gated there. The
constraint kinds sketch mode has no tool for are behind a feature gate, so
a document holds only what the UI can make (`SEED.md` §8.2 rule 7). Sketch
mode is what the agent sees headless: the overlay is a `SketchView` that
`debug_state()` reports and the goldens hold.

## Non-goals

- The forms: the property card, an extrude from the sketch's regions, its
  preview, the gears tab's form, profile picks outside sketch mode, and
  sketches drawn in the 3D view while not in sketch mode (M3's next plan).
- The sketch fillet and chamfer, auto-constrain, the constraint
  descriptions (the backlog line stays), the slot, polygon and
  centre-rectangle templates, the project tool (projection is in the
  document since M2; its tool waits for a scenario), grid snap, splines
  and conics (gated).
- Mouse bindings in the keymap (the backlog line stays); sketch gestures
  are fixed like the viewport's.
- A parameter table or `AddParam` from the UI: a dimension's expression may
  name a parameter the document already has.
- Several sketches open in sketch mode at once; sketch mode is per document
  tab, one sketch each.

## Design deltas

- `docs/UI-RENDERING.md` §Sketch mode: entering (`core.sketch.create` on
  the selected planar face or datum plane, else the world plane of ⚠ OPEN
  1; `core.sketch.edit` on a tree row or a selected sketch), finishing
  (`core.sketch.finish`, issues no command), the camera's turn and its
  return, the tools and their gestures, hit priority, dwell acquisition
  and the modifier that suppresses inference, the dimension field, trim,
  extend, offset and mirror previews, the region shading, the status
  line's DoF and solver state, the gated kinds. §The shell: the Sketch tab
  while in sketch mode, `core.edit.undo` and `.redo`, `core.document.new`.
  §View types: `SketchView` (points, curves in plane-local metres,
  construction flags, constraint glyphs, dimension labels, regions, the
  tool's preview with its snap glyphs and guides, the DoF state per
  entity), the intents `TreeFeatureActivated`, `Sketch*` (pointer events in
  plane-local metres, `DimensionCommitted { constraint, text }`), and the
  layout rule that `arrix-ui` paints the sketch from a plane-to-screen
  transform the app hands it.
- `docs/DATA-MODEL.md` §Sketches and `docs/UI-RENDERING.md` §Sketch mode's
  kinds list: the kinds left without a tool move behind a gate (step 14).
  A document holding one stops deserialising, as a conic does; no schema
  bump unless a fixture or test document holds one (the step checks, and if
  one does, the step becomes a schema bump with its migration).
- `docs/CONCURRENCY-WASM.md` §Budgets: the drag measured after the step 1
  work, and the target turned into a gate if met.
- `arrix-sketch`: the inference engine (`inference`: the snap search,
  curve and relation candidates, the commit rule over
  `Sketch::check_candidate`), ported; the drag's speed-up. Pure, headless,
  wasm-clean (no clock: dwell time comes from egui's).
- `arrix-config`: the sketch commands (`core.sketch.{create,edit,finish}`,
  the tools `core.sketch.{select,line,rectangle,circle,arc,dimension,trim,
  extend,offset,mirror,construction}`, one `core.sketch.constrain.<kind>`
  per kind with a tool, `core.sketch.delete`), `core.edit.{undo,redo}`,
  `core.document.new`, their default keys in the `sketch` and `part`
  contexts; `docs/CONFIG-CATALOG.md` regenerated.
- `arrix-app`: the first commands the UI issues, through the tab's
  session; a per-gesture command counter `debug_state()` reports; sketch
  mode as session state per tab (`SketchMode { feature, draft, tool, drag,
  acquisition }`); the client solves and submits one `SketchEdit` per
  gesture by diffing its draft (ADR-0005).
- `arrix-plugin-api`: no change.
- No ADR is expected: the design docs already decide the solver on the UI
  thread, the commit rule, one command per gesture and the gating. A
  decision that turns out non-obvious in a step (⚠ OPEN 1's answer if it
  is not the default) becomes one then.

## Arris dependencies

None new. Sketch mode is `arrix-sketch`'s; the arrangement that shades the
regions is the ported one until A5 (`region2`) lands (`docs/ROADMAP.md`
§C1 Arris dependencies), and a sketch on a planar face reads its frame
through `ops::query::face_frame`, which the evaluator already calls in
the pinned `arris` 0.5.

## Steps

Complexity: **[1]** routine; **[2]** careful; **[3]** unproven.

- [x] **[3]** Step 1 — The drag inside its budget. Profile
  `crates/arrix-sketch/tests/solver_benchmarks.rs`'s 199-entity drag (34 ms
  median, release) and remove the cost: the dense null-space basis and the
  retract per follow iteration are the suspects
  (`docs/CONCURRENCY-WASM.md` §Budgets). Positions stay bit-identical to
  today's for the existing drag tests unless a test says why not. Done:
  the median at or under 4 ms, then asserted (in release, a margin noted)
  and the number written in §Budgets; or ⚠ OPEN 2.
- [x] **[3]** Step 2 — Inference, ported into `arrix-sketch`: the snap
  search in plane-local metres with a pixel tolerance turned metres,
  point, curve and relation candidates in priority order, and the commit
  rule (bound one at a time, each checked against the sketch with the
  earlier ones in, `Redundant` and `Conflicting` dropped). Starts with the
  probe test (`tests/inference_probe.rs`): the cases the rule turns on,
  established here, not assumed (a redundant alignment under an H lock, a
  tangent snap at the touching point, parallel and perpendicular following
  a turning reference without a flip), and the time of one verdict.
- [x] **[2]** Step 3 — The UI's first document commands.
  `core.edit.undo` and `.redo` (Ctrl+Z, Ctrl+Shift+Z and Ctrl+Y) through
  the tab's session, `core.document.new` (an empty document in a new tab;
  no part until something needs one), and the gesture counter: each
  pointer or key gesture the app handles is opened and closed, the
  commands it submitted counted, and `debug_state()` reports the last
  gesture's count. Test: undo and redo on an opened document, byte for
  byte to the saved files; a no-op gesture counts zero.
- [ ] **[2]** Step 4 — Entering and leaving sketch mode.
  `core.sketch.create`: with a planar face or a datum plane selected, a
  sketch on it; with nothing selected, on the world plane of ⚠ OPEN 1; in
  a document with no part, one `Group` of `AddPart` and `AddFeature`.
  `core.sketch.edit` from a selected sketch or the tree's
  `TreeFeatureActivated` (a double-click on a row; the tree card's first
  intent). Sketch mode is session state: the `sketch` keymap context, the
  Sketch tab's tools enabled, the camera flown to face the plane in
  orthographic projection and returned on `core.sketch.finish` (Escape
  with no tool active), which issues no command. Scenario
  `sketch_enter` (empty document, create, finish): goldens and the counts
  (one command, then none).
- [ ] **[2]** Step 5 — `SketchView` and the overlay. The app builds it
  from the draft solved, with the solver's `Diagnostics` (each entity's
  state: under, fully, over-constrained, conflicting); `arrix-ui` paints
  points, curves (construction dashed), constraint glyphs beside what they
  hold and dimension labels with their leaders, through the transform the
  app hands it, never naming a sketch type. The status line gives the DoF
  and the solver state. Scenario `sketch_view` on the slice's sketch
  (`tests/docs/slice`, opened, the sketch edited): the golden, read.
- [ ] **[2]** Step 6 — Hit testing, hover and selection in the plane: a
  point over a curve over a glyph over a dimension, within a pixel
  tolerance; hover highlights and names the item in the status line;
  click and shift-click select (session state); `core.sketch.delete`
  (Delete) removes the selection and what stands on it as
  `SketchEdit::remove` collects it, one command. Tests: hit priority at
  coincident items; delete then undo restores the sketch byte for byte.
- [ ] **[3]** Step 7 — The line and rectangle tools with inference. The
  pointer's snap is the step 2 search over the draft, drawn as its glyphs
  and guides; a click commits the same inferences by the commit rule; the
  line chains until Escape or a double-click; the rectangle is the
  template (four lines, their coincidences, two horizontals and two
  verticals) plus its corners' inferences. Alignment sources are acquired
  by a dwell on egui's clock (0.3 s); holding Alt suppresses inference.
  Each click that adds geometry is one `SketchEdit`. Tests: what the
  preview shows is what the edit holds (glyph for glyph); a redundant
  candidate is drawn and not committed, a conflicting one neither; one
  command per click; a scenario with a rectangle drawn off the origin and
  one snapped to it.
- [ ] **[2]** Step 8 — The circle (centre, then radius) and arc tools (⚠
  OPEN 3) on the same snap and commit rule; a tangent snap from a line's
  end commits `Tangent` alone where the probe says so. Tests as step 7's.
- [ ] **[2]** Step 9 — The drag. A press on a point or a curve in the
  select tool starts a `DragSession`; each frame steps it to the cursor
  and the overlay follows; the release is one `SketchEdit` diffed from
  the draft before the press, none if nothing moved. A step that does not
  converge leaves the geometry where it was and says so in the status
  line; the branch guard's refusal likewise. Tests: one command per drag,
  zero for a click without movement; undo returns the stored positions
  byte for byte; the frame's solve time reported in `debug_state()`.
- [ ] **[2]** Step 10 — Constraint tools and construction. One
  `core.sketch.constrain.<kind>` per kind that the selection can make
  (coincident, horizontal, vertical, the alignments, parallel,
  perpendicular, equal, concentric, tangent, symmetric, point on, midpoint,
  fix, and the datum kinds by picking the origin or an axis), each enabled
  only for a selection that fits it, checked by `check_candidate` first: a
  redundant or conflicting one is refused with the reason in the status
  line and no command. `core.sketch.construction` (Q) toggles the
  selection's flag. Tests per kind family; one command or none.
- [ ] **[2]** Step 11 — Dimensions as expressions. The dimension tool
  makes distance, horizontal and vertical distance, point–line,
  parallel-lines, radius, diameter and angle dimensions from the picks the
  kind needs, placed by a click; one that would over-constrain is placed
  as a reference dimension (⚠ OPEN 4). A double-click on a label opens
  its field; the text parses as an `Expr` (a parameter the document has
  may be named, resolved from the replica's last `Finished` values), the
  draft solves with it, and Enter or focus loss commits one `SketchEdit`
  (the constraint with its expression, the positions the solve gave); a
  text that does not parse, or a solve that fails, leaves the field
  showing the error and issues nothing; the same value issues nothing.
  Tests: a dimension naming a parameter; an edit of that parameter in the
  document re-evaluates the sketch with no command in sketch mode; undo.
- [ ] **[2]** Step 12 — Trim and extend: hovering a curve previews the
  piece trim removes or the extension extend adds
  (`arrix_sketch::modify` previews); a click commits it, constraints
  transferred, one command. Tests over the ported cases, each one command
  and the preview equal to the commit.
- [ ] **[2]** Step 13 — Offset and mirror: offset of a selected chain by a
  distance typed in a field (an expression) with its preview, one command;
  mirror of the selection about a picked line, one command. Tests as
  step 12's.
- [ ] **[2]** Step 14 — Regions shaded. The draft's arrangement's faces
  (non-construction curves) filled under the curves, a hovered region
  outlined, each region's key in `SketchView` and `debug_state()`, the
  same keys `Ref::Region` stores. Test: the shaded keys equal what
  `core.extrude`'s region input resolves for the saved sketch; a hole drawn
  inside a region splits it in the view as the evaluator does.
- [ ] **[2]** Step 15 — The kinds without a tool gated: the enabled list
  of `docs/UI-RENDERING.md` §Sketch mode checked against steps 10 and 11,
  each kind left over moved behind a gate (`unreached`, off by default, its
  code and tests in the all-features build, as `conics`); every test
  document and fixture checked to hold none (if one does, a schema bump
  with its migration, ⚠ named in the step's commit). The docs' lists
  updated to what is reachable.
- [ ] **[2]** Step 16 — The acceptance scenario `sketch_accept`, below.

## Acceptance

`cargo test -p arrix-app --test visual sketch_accept`, driven through the
UI by pointer and keys only, on a document opened from a new test document
`tests/docs/sketch-mode` (an empty part and one parameter `width = 60 mm`):

1. `core.sketch.create` with nothing selected: sketch mode on the default
   plane, the camera square to it, orthographic. One command.
2. A rectangle drawn with one corner snapped to the origin: the snap's
   glyphs in the golden equal the constraints the edit holds. One command.
3. Its bottom edge dimensioned and the label set to `width`; its height
   set to `40 mm`; the sketch fully constrained but for what remains, the
   DoF in the status line. One command each.
4. A circle inside it, drawn, its centre dragged (one command on release,
   none for a click without movement), dimensioned `10 mm` radius.
5. A line across the rectangle, trimmed back to it, and a construction
   line mirrored across: one command each.
6. The regions shaded: the golden lists their keys (the rectangle less the
   circle, the circle).
7. Finish: no command. Save; `arrix eval` on the saved document gives the
   same region keys and a sketch with no diagnostic.
8. The gesture count over the flow: every gesture exactly one command, or
   none where nothing changed. Undo of every command, in reverse, returns
   the document byte for byte to each earlier state.

The goldens (`crates/arrix-app/tests/snapshots/`) are read before they are
accepted, and the step 1 drag benchmark asserts its budget.

## Docs to update on completion

- `docs/UI-RENDERING.md` §Sketch mode: present tense for everything built
  (entering, finishing, tools, hit priority, dwell, the modifier, the
  dimension field, previews, regions, the status line, the drag inside its
  budget); §The shell: the Sketch tab's tools, undo and redo, new
  document; §View types: `SketchView` and the new intents; §The viewport
  §Overlays: the sketch overlay as built.
- `docs/DATA-MODEL.md` §Sketches: the reachable kinds and the gate.
- `docs/CONCURRENCY-WASM.md` §Budgets: the drag measured and gated.
- `docs/CONFIG-CATALOG.md`: regenerated (`UPDATE_SNAPSHOTS=1 cargo test
  -p arrix-cli catalog`).
- `docs/ROADMAP.md` M3 status line: sketch mode landed, and what M3
  continues with (the forms).
- `docs/BACKLOG.md`: the drag-budget line removed (this plan absorbs it);
  lines added for the templates, the project tool and grid snap left out
  here.
- `AGENTS.md` current state and §Commands: the sketch scenarios.

## Open questions

- ⚠ OPEN 1: the plane `core.sketch.create` uses with nothing selected.
  Default proposed: world XY (Top), with the user re-picking by selecting a
  face first; the alternative is a plane chooser showing the three world
  planes in the viewport to click (more UI, one more gesture). Human, by
  step 4.
- OPEN 2 (closed at step 1): the grid reaches 4 ms (0.38 ms median,
  gated). Finding: a sketch of mostly nonlinear rows is still over (a
  200-line zigzag, 300 ms a frame, from 7.2 s), now a backlog line; the
  positions differ from the unreduced solver's by round-off (about 1e-8 m),
  not bit for bit, which `solver/tangent.rs`'s test pins.
- Finding (step 2): the probe holds every case of the commit rule as
  established before (the redundant alignment under an H lock, `Tangent`
  alone at the touching point, parallel and perpendicular without a flip),
  and one verdict costs 0.11 to 0.58 ms in release (§Budgets). The grid
  pass and its glyph were left out of the port (grid snap is a non-goal),
  and the angle tolerance is a constant, `ANGLE_TOL`.
- ⚠ OPEN 3: the arc tool's gesture: centre, start, end; three points
  (start, end, through); or both, the second sharing the button. Default
  proposed: three-point, as the most common, with tangent arcs left out.
  Human, by step 8.
- ⚠ OPEN 4: a dimension that would over-constrain: placed as a reference
  dimension (shown, not driving), or refused with the reason. Default
  proposed: reference, as the ported code did. Agent, by step 11, unless
  the human objects.
