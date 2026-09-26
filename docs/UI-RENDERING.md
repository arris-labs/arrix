# UI and rendering

The egui shell, the ribbon and palette, declarative plugin UI, sketch mode,
the viewport and picking, and how the agent sees all of it headless. Crates:
`arrix-ui` (widgets over view types), `arrix-viewport` (wgpu), `arrix-app`
(the shell and the translation to and from the document). The stack is
egui/eframe/wgpu (`SEED.md` §9), pinned to one egui minor for the workspace.
Nothing here is built yet: each section becomes true as C1 lands, and the
commit that builds it keeps it true.

## Principles

1. **One gesture = one command** (`SEED.md` §8.2). A field commits on
   focus loss or Enter, a drag on release. A commit equal to the current
   state issues no command. Previews during a gesture are session state.
2. **What a snap shows is what it commits.** The inference drawn and the
   constraint written are the same object.
3. **The UI never blocks.** It submits commands and draws what the latest
   events say. A slow evaluation shows progress on the features it is
   working on; the rest of the UI stays live.
4. **The UI is a projection of the document.** Immediate mode redraws
   from view types each frame; there is no retained widget state that
   could disagree with the document.
5. **The agent sees the UI itself** (§Visual debugging).

## The shell

```
┌──────────────────────────────────────────────────────────────────────┐
│ ribbon: Sketch │ Part │ <plugin tabs> …                     search ⌘K │
├───────────────┬──────────────────────────────────────────┬───────────┤
│ feature tree  │                                          │ properties│
│ (per part)    │               viewport                   │ (form of  │
│               │                                  ViewCube│ the edited│
│               │                                          │ feature)  │
├───────────────┴──────────────────────────────────────────┴───────────┤
│ status: generation, evaluation progress, diagnostics count, units    │
└──────────────────────────────────────────────────────────────────────┘
```

- **The command registry** in `arrix-app` holds every user action as a
  `CommandSpec`: id (`core.extrude`, `gears.spur`), title, icon, shortcut,
  ribbon placement, and whether it applies in the current session state.
  The ribbon, the command palette, the context menus and the shortcut map
  all read it. A plugin's ribbon group and commands land in the same
  registry; **installing a plugin adds a ribbon tab, never a mode**
  (`SEED.md` §5).
- **The command palette** (`Ctrl+K`) searches every registered command by
  title and id, the plugins' included.
- **The feature tree** shows each part's history with state per feature:
  ok, failed (with the diagnostic), frozen or frozen-stale (with the
  reason), suppressed, rolled back. A rollback bar is dragged by one
  command on release.
- **The property panel** is the form of the feature or record being
  edited, rendered from its `Form` (§Declarative plugin UI). Built-in
  features use the same renderer, so the renderer is proven on every
  feature, not only on plugins.
- **Diagnostics** show on the feature, in the status bar count, and as
  highlighted references in the viewport. A lost reference opens a re-pick
  with its candidates highlighted.
- Display units are mm and deg by default, a document preference
  (`docs/DATA-MODEL.md` §Parameters and expressions). Every numeric field
  accepts an expression.

## View types

`arrix-ui` renders view types and returns intents; it never names a
document or sketch type (`docs/ARCHITECTURE.md` §Crates and the layer
rule).

- **View types** are plain data built by `arrix-app` from a document
  snapshot, the latest events and session state: `TreeView`, `FormView`,
  `RibbonView`, `PaletteView`, `DiagnosticsView`, `SketchView`,
  `SceneView`. They are serialisable, which is what makes them testable
  and what a thin client would receive.
- **Intents** are what a user did, in view terms: `UiIntent::FieldCommitted
  { form, field, text }`, `RibbonClicked { command }`, `Picked { name }`.
  `arrix-app` turns an intent into a `CommandEnvelope`, or into a change of
  session state (selection, camera), which issues no command.

## Declarative plugin UI

Plugins contribute UI as data (`docs/PLUGINS.md` §6. UI). The core renders
it.

| Kind | Rendered as |
|---|---|
| Ribbon group | a group in the plugin's tab: buttons bound to the plugin's commands |
| Form | the property panel: fields (quantity with unit, integer, choice, toggle, text, reference pick), lists, buttons, per-field validation messages |
| Panel | a dockable panel made of form widgets |
| Context-menu entry | an item on a right-click over the reference kinds it declares |
| Drawables | viewport overlays: points, lines, frames, labels, meshes with a per-vertex colour field and a legend, gizmos with typed drag handles (linear along an axis, angular about one, planar) |

A gizmo's drag is a preview until release, and the release is one command
built by the plugin from the handle's final value. Validation runs on the
form's own rules while typing and in the plugin on commit; a rejected
commit leaves the field showing the error, not the old value.

Tier 0 drawing its own egui is an escape hatch decided by ADR when a real
plugin needs it, not before.

## Sketch mode

Sketch mode edits one `core.sketch` feature. It is session state: entering
it issues no command, and the camera turns to face the plane in
orthographic projection.

- **The solver runs on the UI thread**, the one sanctioned exception to
  "the kernel never runs on the UI thread" (`SEED.md` §6.1): sketches are
  small, and a drag must follow the cursor in the same frame. It has a
  budget per frame (`docs/CONCURRENCY-WASM.md` §Budgets); a solve that
  does not converge within it leaves the geometry where it was and says
  so.
- **The solver** is ported (`SEED.md` §8.1): DogLeg by default and
  Levenberg–Marquardt selectable, an analytic sparse Jacobian, independent
  subsystems solved apart, DoF, redundancy and conflict from one rank
  analysis. Drag steps within the constraints' null space and a branch
  guard refuses a step that would flip an angle, a tangency side or an
  arc's sweep.
- **Inference is one engine** in `arrix-sketch`: a search over candidate
  snaps in priority order, in plane-local metres with a tolerance in
  pixels. Its result is drawn as glyphs and guides, and the click commits
  the same inferences. Every candidate is checked against the sketch first:
  a redundant one is drawn but not committed; a conflicting one is neither.
  Alignment sources are acquired by dwell; a modifier suppresses inference.
- **A sketch edit is one command.** Drawing a line with its inferred
  coincidence and horizontal is one `SketchEdit`; a drag is one on release;
  trim, extend, offset and mirror are each one.
- **Regions** are shaded from the sketch's planar arrangement, and the
  region a profile pick stores is the one shaded (`docs/DATA-MODEL.md`
  §Sketches).
- **Constraint kinds reachable from the UI are the only ones a document
  holds.** The solver's other kinds stay behind feature gates until a
  scenario reaches them (`SEED.md` §8.2 rule 7).

## The viewport

`arrix-viewport` draws the scene into an offscreen target inside the egui
frame, through an `egui_wgpu` paint callback (`prepare` uploads, `paint`
draws). Ported, reviewed on the way in (`SEED.md` §8.1).

- **Scene per body.** A `SceneView` is a list of bodies, each keyed by its
  slot, with its mesh, transform, visibility and bounds. Updating one body
  re-uploads only it. The upload is behind a payload trait so the
  bookkeeping is tested without a GPU.
- **Meshes** arrive in `EvalEvent`s as `RenderMesh`: f32 positions
  relative to the body's local origin (the kernel works in f64; the cast
  happens where the buffer layout is known), normals, face ranges and edge
  polylines, each range carrying its entity's `PersistentName`.
  Tessellation comes in two passes: a coarse mesh first so the body shows
  at once, then a refined one.
- **Picking by ID buffer.** Faces, edges and vertices draw a `u32` id into
  an `R32Uint` target: kind in the top bits, the body's scene index, the
  entity's index in its mesh. A pick reads a small region around the
  cursor back asynchronously (`map_async`, polled each frame, never
  blocking), ranks vertex over edge over face and then by distance, and
  resolves to the `PersistentName` through the mesh's range table. The
  path is the same on native and wasm. Picking needs nothing from the
  document, so it stays client-side in remote mode.
- **Hover and selection** restyle index ranges (the face's fill, its
  boundary edges as an outline), rather than a screen-space pass.
- **Camera**: orbit about a target, pan, zoom to cursor, perspective or
  orthographic, standard views, animated transitions that tests can skip.
  **Z-up, right-handed** (`SEED.md` §8.2). The ViewCube lives in
  `arrix-ui` and is painted with egui's painter; it returns an action
  (select a view, orbit, home, toggle projection).
- **Overlays**: the sketch (points, curves, constraint glyphs,
  dimensions), plugin drawables, diagnostics highlights, frozen badges on
  bodies from frozen features.

## Visual debugging

The agent looks at the UI itself, headless; the human is never asked to
describe the screen (`SEED.md` §8.2 rule 6). Ported harness, reviewed on
the way in.

- **Snapshot scenarios** (`crates/arrix-app/tests/visual/`) drive the real
  app through `egui_kittest` with a real wgpu renderer at a fixed size,
  pump frames until evaluation is idle, and compare against one golden per
  scenario, `crates/arrix-app/tests/snapshots/<name>.json`, holding two
  things: the `debug_state()` at that moment, and the **coarse frame**.
  Scenarios run one at a time: concurrent software devices are not safe.
- **No image is committed** (ADR-0001). Goldens are text; images are build
  output, re-rendered from any commit on demand.
- **The coarse frame** is the rendered frame reduced to a grid of 16×16
  pixel cells, each the cell's mean RGB, written as one hex string per grid
  row (90×56 cells at 1440×900, about 30 KB of text that deltas well in
  git). A scenario fails when more than its budget of cells differ from the
  golden by more than the per-cell tolerance; the cell size, tolerance and
  budget are the harness's, overridable per scenario. The failure names the
  differing cells as pixel rectangles. It catches layout, placement, colour
  and gross rendering changes; a change smaller than a cell (a 1-px
  outline) is below it, and text is `debug_state()`'s to catch.
- **Images on demand.** Every run writes the full frames to
  `target/snapshots/<name>.png`. `scripts/snapshot-baseline [<rev>]`
  (default `HEAD`) renders the same scenarios at that revision in a
  temporary worktree and writes `<name>.before.png` and a
  `<name>.diff.png` beside them. CI uploads all three as artifacts when a
  scenario fails.
- **`debug_state()`** is JSON of what the app drew and why, floats rounded
  to six places: the camera (eye, target, projection, matrices), the
  evaluation (generation, each feature's state and diagnostic), the bodies
  drawn (triangle, face, edge and vertex counts, bounds), selection and
  hover as persistent names, the open sketch (points in plane and screen
  coordinates, constraints, DoF, solver status), the ribbon and palette,
  and each plugin form and panel rendered. It is how the agent tells which
  number is wrong, not only that a pixel is.
- **Deterministic frames.** No wall clock reaches a drawn pixel in a test:
  animations are skipped, the frame-time readout is off, and evaluation is
  waited for, not timed.
- **The scratch render** (`cargo test -p arrix-app --test visual_scratch`)
  draws a state to `target/visual-scratch/scratch.{png,json}` and compares
  against nothing: the agent's way to look before it pins a scenario.
- **Refreshing a golden** (`UPDATE_SNAPSHOTS=1`) is done only for an
  intentional UI change, after reading the before, after and diff images
  rendered on demand and the golden's text diff, and the commit says why
  (`.agents/rules/git.md`).
- **Pitfalls** the harness encodes, so no one relearns them: a frame step
  without a render does no GPU work; a click is one event per rendered
  frame; evaluation is asynchronous, so settle before asserting; never
  start a camera animation in a test.
- The `visual-debug` skill, ported with the harness, is the agent's
  how-to.

## Open questions

None yet.
