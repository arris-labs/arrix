# Plan: viewport-port

- Started: 2026-10-07
- Cycle: C1 M3 (`docs/ROADMAP.md` §M3: "`arrix-viewport` ported and reviewed")
- Idea: none. Verbatim from the human: "the viewport port, UI should be
  minimal and panels show only on-demand so when user opens app it has
  minimal set of ui elements. E.g. object tree should float on top of main
  3d view instead on full panel"; then: "there should be only 2 panels
  docked for now: ribbon on top and tabs of opened projects on bottom +
  status line below it for showing different help text there"

## Goal

The app opens documents (`arrix-app tests/docs/slice tests/docs/gear-on-plane`),
one tab each, and shows the active one's bodies in a wgpu viewport that
fills the window between the docked bars: coarse
meshes first, then refined, from tessellation on the evaluator. The camera
orbits, pans and zooms to the cursor, Z-up, and the ViewCube turns it to a
standard view. A click picks a face, edge or vertex through the ID buffer
and the selection is a `PersistentName`; hover and selection are restyled
index ranges. The shell is minimal, with exactly two docked bars: the
ribbon on top, and at the bottom the tabs of the open documents with a
status line under them that shows help text for whatever is under the
pointer. On an empty start the window holds those and the viewport,
nothing else. The feature tree is a card floating over the viewport, shown
once there is something in it, with each feature's state; the property
card floats on the other side and appears only on demand. Each tab keeps
its own camera, selection and cards. Picking, hover, the camera, the active
tab and panel visibility are session state and issue no command.

## Non-goals

- Sketch mode, sketch overlays and inference (M3's sketch-mode plan).
- The property card's content: forms and the declarative renderer (M3's
  forms plan). This plan places the card and shows it on demand, empty.
- Editing from the tree: rollback bar, suppress, reorder, rename, context
  menus. The tree shows; it does not edit.
- Opening or saving through the UI (a file dialog, recent files): a path on
  the command line or `ArrixApp::open` in a test is enough here.
- Plugin drawables, gizmos, diagnostics highlights in the viewport, the
  frozen badge on bodies (later M3 plans; the frozen state shows in the
  tree here).
- Per-face re-tessellation (A9), cancellation of tessellation (A3).
- Mouse bindings in the keymap: fixed mouse gestures now, a backlog line.
- Document tabs beyond open, switch and close: reordering by drag, a new
  document, an unsaved-changes prompt (nothing in this plan issues a
  command, so nothing is unsaved), split views of two tabs.

## Design deltas

- **ADR-0017, the minimal shell** (step 1): exactly two docked bars, the
  ribbon on top and, at the bottom, the document tab strip with the status
  line under it; the viewport fills everything between them; the tree and
  the property panel are overlay cards on the viewport, never docked
  panels; every card is hidden until it has something to show or is asked
  for. The status line is help text on the left (what the hovered button,
  palette row or viewport entity is and does, its keys, else the gestures
  the viewport takes) and the diagnostics count and units on the right.
  **Several documents open at once**, a tab each with its own session,
  scene, camera, selection and cards; the active tab is session state.
  M3's in-list names one document, so the ADR names the scenario tabs
  unlock: a user keeps a plugin's test document open beside the part it
  goes into and switches between them with a click, each where they left
  it. `ui.panel.tree.visible` and
  `ui.panel.properties.visible` (bool) become `ui.panel.tree` and
  `ui.panel.properties`, each `"auto" | "shown" | "hidden"` (default
  `"auto"`); the old keys are a diagnostic naming the new one (config has
  no users yet, so no migration beyond the message). The toggle commands
  flip the session's visibility over the setting. Supersedes nothing;
  amends the §The shell diagram, which was never an ADR.
- `docs/UI-RENDERING.md` §The shell: the diagram and the regions; the
  layout is a function of the window size *and* which cards are showing;
  cards are `egui::Area`s anchored to the viewport rect (fixed, not
  draggable, so the layout stays deterministic), and a pointer over a card
  never reaches the viewport. §View types: `ShellLayout` gains the cards
  as `Option<Rect>` and the tab strip, and loses the docked tree and
  properties; new `DocumentTabsView` (title, path, active),
  `StatusView.hint`, `SceneView`, `CameraView`, `ViewCubeAction`,
  `UiIntent::{Picked, Hovered, DocumentTabSelected, DocumentTabClosed}`;
  `TreeView` gains features with their states.
- `RenderMesh` in `arrix-core` (plain data: f32 positions about the body's
  local origin, per-corner normals, triangles, face ranges, edge
  polylines, vertex points, each range with its `PersistentName`), so
  `arrix-viewport` names no doc or kernel type.
- `arrix-kernel`: `Kernel::tessellate(body, detail) -> RenderMesh`, a
  recorded call like every other (`SEED.md` §6.6). Built in step 3 with
  a `MeshDetail` (coarse, refined) rather than a chord: the kernel turns
  it into a chord from the body's box, which only it can read, and
  records both; `RenderMesh` carries its `origin` and `chord`.
- `arrix-doc`: `SessionEvent::Meshed { generation, part, slot, detail:
  Coarse | Refined, mesh }`. The evaluator meshes a part's *final*
  bodies (each slot's last version, not consumed) once the part is
  evaluated, coarse for all of them, then refined; a cached version keeps
  its meshes. This refines `docs/CONCURRENCY-WASM.md` §Tessellation
  ("after each body"), which would mesh intermediate versions nobody sees.
  Events are not persisted: **no schema bump**.
- `arrix-viewport`: the ported scene, camera, renderer and picking on
  `glam` (the source used `cgmath` and a kernel crate; neither comes in).
  New deps: `bytemuck`, `glam` (workspace), `wgpu` through `egui-wgpu`.
- `arrix-ui`: the ported ViewCube, painted with egui's painter. It is
  ported from a different source than the viewport, the better of the two,
  which brings step arrows, corner axes and a projection button; it reads
  the viewport's `ViewOrientation` and `Projection` (the layer rule allows
  `arrix-ui` on `arrix-viewport`) and returns an action.
- `arrix-config`: commands `core.view.home`, `core.view.{top,front,right,
  iso}`, `core.view.projection.toggle`, `core.selection.clear` (Escape in
  `part`), `core.document.{next,previous,close}` (`ctrl+tab`,
`ctrl+shift+tab`, `ctrl+w`); the panel settings above. Catalog regenerated in the step that
  changes it.
- `arrix-plugin-api`: unchanged.

## Arris dependencies

All present in the pinned `arris` 0.5:

- `arris::mesh::tessellate_with(&Model, Body, &MeshRequest, &Control)` with
  `MeshRequest::new(chord).with_corners()`: the watertight buffer, its
  `TriMesh::faces()` (`FaceRange { face, triangles }`), `edges()` and
  `edge_indices()` (`EdgeRange { edge, indices }`), and `corners()` for
  sharp per-face normals (Arris ADR-0012).
- Vertex positions through the kernel's existing `vertex_point` path.
- Not needed here: A9 (per-face tessellation) and A3 (cancellation); a
  superseded generation's meshes are dropped by their generation, never
  drawn.

## Steps

- [x] **[1]** 1 — ADR-0017, the minimal shell: the two docked bars, the
  cards and their modes, the settings rename, document tabs with the
  scenario they unlock, and `auto` as decided below.
- [x] **[2]** 2 — The minimal shell on the empty app: the ribbon docked on
  top, the (empty) document tab strip and the status line docked at the
  bottom, the viewport central between them; the status line's hint from
  the hovered ribbon button or palette row (title and keys), else a
  start hint; tree and property cards as anchored overlays,
  `auto` hiding both while there is nothing to show; the settings renamed
  with the old keys' diagnostic; the toggles flip session visibility;
  `ShellLayout` reports the cards and the tab strip. Goldens `empty_shell`, `light_theme`,
  `palette_open`, `configured_shell` re-pinned after reading
  before/after/diff (`scripts/snapshot-baseline`); a scenario with both
  cards toggled on. Catalog regenerated.
- [x] **[3]** 3 — Probe and build `Kernel::tessellate`: `RenderMesh` in
  `arrix-core`, every face, edge and vertex range named from `BodyNames`,
  the call recorded. Tests on the slice's plate and gear: closed, every
  range named, two runs byte-identical, mesh volume within the chord's
  bound of `mass_properties`. Establishes the coarse and refined chords
  (relative to the bounds' diagonal) and writes them down.
- [x] **[2]** 4 — The evaluator meshes a part's final bodies:
  `SessionEvent::Meshed`, coarse then refined, cached versions reused,
  consumed and intermediate versions never meshed. Session tests on the
  slice's stream; the JSON round-trip test covers the new event; the first
  coarse mesh after a width edit is measured and printed (budget 150 ms,
  reported).
- [x] **[1]** 5 — `arrix-viewport` scene and camera, GPU-free: `Scene` keyed
  by part and slot behind the payload trait (re-upload only what changed),
  `OrbitCamera` (orbit, pan, zoom to cursor, perspective/orthographic,
  standard views, animations skippable) on `glam`, Z-up; the source's
  tests ported.
- [x] **[3]** 6 — The wgpu renderer: pipelines, shaders, the `egui_wgpu`
  paint callback into an offscreen target in the viewport rect. Proven in
  the scratch render with a fixture `RenderMesh` under lavapipe: visible,
  and two renders pixel-identical. The wasm build stays green.
- [x] **[2]** 7 — The app opens a document: `ArrixApp::open(source)` hosts a
  `LocalSession`, drains its subscription each frame without blocking,
  keeps a replica and feeds `Meshed` into the scene; `arrix-app <path>` on
  native; the camera frames the bodies on the first mesh. `debug_state()`
  gains `evaluation`, `bodies` and `camera`. Scenario `slice_viewport`
  (settled on the refined meshes) pinned after reading its frame.
- [x] **[2]** 8 — Document tabs: several paths open several tabs, each its
  own `LocalSession`, scene, camera, selection and card visibility;
  switching and closing by click and by `core.document.*`; closing the
  last tab returns to the empty start. Scenario: the slice and the gear
  opened, the second tab active, then the first again with its camera as
  it was.
- [x] **[2]** 9 — The tree card from the replica: parts and features with
  their states (ok, failed, frozen, frozen-stale, suppressed, rolled back),
  shown in `auto` once a part exists. Scenario: the slice opened without
  `gears`, the gear frozen in the tree.
- [x] **[3]** 10 — ID-buffer picking: the `R32Uint` target, the async
  readback polled per frame, ranking vertex over edge over face then by
  distance, resolved to a `PersistentName`; `UiIntent::Picked`. GPU-free
  tests for the id encoding and ranking; a scenario clicks the plate's top
  face and `debug_state().selection` holds its name; a click on the tree
  card picks nothing.
- [x] **[2]** 11 — Hover and selection as restyled index ranges (fill, and
  the boundary edges as outline); `core.selection.clear`; the generation
  is unchanged by every pick, hover and clear (asserted); the status
  line's hint names the hovered entity and what a click does. Scenario
  with a hovered edge and a selected face.
- [ ] **[2]** 12 — The ViewCube ported into `arrix-ui` from its own
  source, not the viewport's (already on `glam`): the chamfered cube's
  faces, edges and corners, drag to orbit on the cube, the step arrows,
  the corner axes and the projection button, painted top-right over the
  viewport and returning `ViewCubeAction` (it never writes the camera
  itself); its tests ported. Mouse orbit, pan and zoom to cursor; the
  `core.view.*` commands with default keys. Scenario: the
  ViewCube's top face clicked, the camera's matrices in the golden.
  Catalog regenerated.
- [ ] **[2]** 13 — The acceptance scenario `viewport_accept` and the
  budgets: a UI frame with the slice on screen measured and printed
  (16 ms, reported), the numbers written to
  `docs/CONCURRENCY-WASM.md` §Budgets.

## Acceptance

`cargo test -p arrix-app --test visual viewport_accept`, on lavapipe: the
empty start first, showing only the ribbon, the viewport, the empty tab
strip and the status line with its start hint (`layout` has no cards);
then the slice and the gear opened as two tabs, the slice active; settled,
both of the slice's bodies
drawn with refined meshes (`bodies` counts and bounds in the golden), the
tree card showing the slice's features all ok and no property card; a
click on the plate's top face selects its persistent name; a ViewCube
click turns the camera to the top view; Escape clears the selection; the
gear's tab shows its own body and tree, and back on the slice's tab the
camera is still the top view; each document's generation unchanged
throughout. Plus the whole gate, the wasm
build included.

## Docs to update on completion

- `docs/UI-RENDERING.md` §The shell — the minimal shell: the new diagram,
  the two docked bars, the document tabs, the status line's hint, cards
  over the viewport, the panel modes, ADR-0017.
- `docs/UI-RENDERING.md` §View types — `ShellLayout` cards and tab strip,
  `DocumentTabsView`, `StatusView.hint`, `SceneView`,
  `CameraView`, `TreeView` features and states, `ViewCubeAction`,
  `Picked`, `Hovered` and the tab intents; "Built so far" updated.
- `docs/UI-RENDERING.md` §The viewport — as built: what was ported and
  what changed on the way in (glam, no kernel dependency), the chords,
  the mouse gestures.
- `docs/UI-RENDERING.md` §Visual debugging — `debug_state()`'s new fields
  (evaluation, bodies, camera, selection, hover); viewport pitfalls found.
- `.agents/skills/visual-debug/SKILL.md` — opening a document in a
  scenario, settling on refined meshes, clicking in the viewport.
- `docs/CONCURRENCY-WASM.md` §Tessellation (after the part, final bodies
  only) and §Budgets (the measured numbers).
- `docs/ARCHITECTURE.md` §The kernel choke point — `tessellate` built;
  the protocol's event list gains `Meshed`.
- `docs/DATA-MODEL.md` §Evaluation — the `Meshed` event beside
  `Evaluated`.
- `docs/ROADMAP.md` M3 status line — plan 2 (`viewport-port`), ADR-0017.
- `docs/ARCHITECTURE.md` §Threads — one session per open document.
- `AGENTS.md` current state — the viewport landed; next M3 plan.
- `docs/BACKLOG.md` — mouse bindings in the keymap.

## Open questions

- Decided by the human (2026-10-07), formerly open question 1: two docked
  bars only, the ribbon on top and the document tabs with the status line
  under them at the bottom; the status line carries help text.
- Settled in step 2 (agent), formerly ⚠ OPEN 1: heights. As proposed: the ribbon stays 88
  points, the tab strip 24, the status line 20; the tab strip is shown
  empty on the empty start rather than hidden, so the layout doesn't jump
  when the first document opens.
- Decided by the human (2026-10-07), formerly open question 2: the tree
  card's `auto` shows it whenever the active document has a part, so it is
  hidden on the empty start. The property card's `auto` is "while a
  feature is being edited", which no step here reaches, so it stays hidden
  in this plan unless toggled.
- ⚠ OPEN 3 (agent, by step 12): mouse gestures. Proposed: middle drag
  orbits, shift + middle drag pans, wheel zooms to the cursor, left click
  picks; right drag orbits too, for a touchpad-less mouse without a usable
  middle button.
- Settled in step 3 (agent), formerly ⚠ OPEN 4: the chords are 1e-3
  (coarse) and 1e-4 (refined) of the diagonal of the box of the body's
  edges and vertices (`MeshDetail::chord_fraction`): a circle the size of
  its body is about 42 segments coarse and 130 refined. The slice's
  counts barely move with the chord, since the gear's flanks are short
  arcs already: the gear is 1,756 triangles coarse and 1,996 refined, the
  plate and gear joined 1,772 either way, each mesh 8–10 ms in the test
  profile with Arris's debug check, far inside the 150 ms budget, so
  the budget did not constrain the choice. The box is the edges' rather
  than the faces' (cheap, and exact for every extrude); a face that bulges
  past its edges (a sphere, a revolved torus) gets a coarser chord than its true box would
  give, which is a resolution, not a correctness, matter.
- Found in step 3: the slice's gear can only be built where both the
  kernel and `gears` may be used, so the tessellation tests are in
  `arrix-cli` (`tests/tessellate.rs`), as `gear_naming.rs` is; the
  kernel's own tests cover a mesh record's fixture export.
- Found in step 4: the slice ends with one final body, the pocketed plate:
  the gear is consumed by the join and the plate's other versions are
  intermediate, so `Meshed` carries one body there, not two (the
  acceptance's "both of the slice's bodies" reads as that one). The first
  coarse mesh after a width edit was 227 ms from the submit to the event
  in the test profile (Arris's debug check on), which includes
  re-evaluating the plate, join and pocket, over the 150 ms budget; it is
  reported, to be measured again in step 13 on the gate's profile. Meshing
  starts when the whole evaluation has finished, not part by part, and
  `Meshed` carries `Result<RenderMesh, Diagnostic>` so a failed
  tessellation is shown rather than dropped.
- Found in step 5: the camera comes in with the sketch-plane machinery
  left out (a sketch-mode plan's), its tests ported, and the zoom range tied
  to the depth range with `set_depth_range_for(radius)`, which step 7 calls
  on every framing so a 40 mm plate is not clipped by a 1 cm near plane.
  `Scene::set_body` skips the upload when the body already holds that very
  mesh (a fingerprint of what it draws), since the evaluator sends every
  final body's mesh again on each generation; the viewport's `BodyKey` is
  part, feature and slot in `arrix-core` types, so it names no doc type.
  The camera is clocked by `web_time`, new in the workspace.
- Found in step 6: the renderer draws on the empty start too, its
  background gradient keyed to the view direction, so the five shell
  goldens were re-pinned (their diffs only inside the viewport's rect).
  Edges draw as lines over the faces, moved toward the eye along their view
  ray by 5e-4 of their depth: a fixed clip-depth offset let hidden edges
  poke past corners, since perspective depth is that uneven at near 0.01.
  `ArrixApp::show_mesh` is the one way a mesh reaches the scene; step 7's
  `Meshed` handling calls it. The proof is `viewport::a_mesh_draws_and_draws_identically`
  on a hand-made fixture box (`tests/visual/fixture.rs`), not a golden:
  the first viewport golden is step 7's `slice_viewport`.
- Found in step 7: `ArrixApp::open(path, source)` and `open_path` (a
  directory or an `.arrx`, read in `document_files.rs`, a new place the
  wasm lint permits to name `std::fs`). A document is settled once its
  evaluation has finished and every coarse mesh has its refined one
  (counted per generation); a part with no final body never says so, so
  that case settles one wall-clock second after `Finished`, on
  `web_time`, since egui's clock runs ahead of it under kittest. Scenarios
  poll `debug_state().evaluation.settled` (`harness::open_settled`),
  stepping rather than running because a pending document keeps asking
  for repaints. Bodies the new generation no longer has leave the scene
  when its refined meshes start; a generation with *no* final body
  leaves the old ones until the next one with a body (a limit, to be
  fixed with the evaluator saying "meshed" explicitly, if it matters).
  The new `debug_state()` fields are omitted when empty, so the shell
  goldens are unchanged.
- Found in step 8: the keymap's `text` context was on whenever *any*
  widget had keyboard focus (`egui_wants_keyboard_input`), so a tab reached
  with Ctrl+Tab silenced every shortcut, `ctrl+w` included; it is now on
  for a focused text edit only. Each document keeps its own card toggles
  (`OpenDocument::cards`), the empty start's are the session's. The three
  `core.document.*` commands join every keymap golden and the palette's
  list, which pushed three creation rows out of `configured_shell`'s
  visible results (the only pixels that moved). A tab shows its path as
  given on the command line; scenarios open relative paths so goldens hold
  none of this machine's.
- Found in step 9: a session over a document that holds a frozen result
  panicked on subscribing, since `Opened` sent every file as UTF-8 text
  and a blob is not; no earlier test subscribed to a saved slice. `Opened`
  now carries `blobs` (bytes, omitted when empty) beside the JSON `files`,
  with a regression test in `arrix-cli`'s `slice.rs`: an `arrix-doc`
  change this plan did not list, and the reason the tree scenario can
  open a saved slice. The scenario opens the slice, saves it
  (`ArrixApp::saved`, a hook until the UI saves) and opens that without
  `gears`. `FeatureState` has the six states of the step plus
  `evaluating`, for a feature with no outcome yet.
- Found in step 10: an id is the kind in two bits, the body's index among
  the visible bodies in ten, the entity's index in twenty; the body index
  comes from the per-draw uniform (now a model matrix and pick bits), so
  hiding a body re-uploads nothing. A pick resolves only against the very
  meshes its pass drew (each body's mesh fingerprint is taken with it),
  since a refined mesh can land before the read-back does; a later click
  replaces a pick still on its way. The region is 7 × 7 pixels. The
  viewport now senses clicks, which makes it focusable, so the Ctrl+Tab
  presses of `document_tabs` leave egui's focus on it rather than on a
  tab's ×: that golden lost the ×'s focus ring and was re-pinned after
  reading the diff. Scenarios find a point with `ArrixApp::screen_point`,
  click with rendering on every step (`set_render_every_step`), and wait on
  `debug_state().picking`; `selection` is the active tab's, by name. The
  slice's plate top resolves to `face:sweep.<plate>.end-cap`; step 11's
  hover reuses this pass.
