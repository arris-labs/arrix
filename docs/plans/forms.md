# Plan: forms

- Started: 2026-10-08
- Cycle: C1 M3 (the property panel rendered from forms, the gear's
  included, and diagnostics; the extrude preview of M3's accept)
- Idea: none; verbatim from the human: "forms". Absorbs the backlog line
  "Choice fields for plugin feature types" (plans/c1-m1-document step 9).

## Goal

Every feature type with a `<type>.create` command, `gears.spur` included,
is created and edited through the property card, rendered by one generic
renderer from the type's spec: quantity and count fields that take
expressions, choices, a name, and reference fields filled by picking in
the viewport, the tree or a finished sketch. A create form is a draft
whose result is previewed in the viewport and committed by OK as one
command, Cancel issuing none; an edit form, opened from the tree, commits
each field as one command. A field whose text the form's own rules refuse
keeps the text and shows why, issuing nothing. The form shows its
feature's state and diagnostic, a lost reference in its field with its
candidates highlighted for a re-pick, and a frozen feature's values read
only with the reason. Choices are declared in the plugin API, so a
plugin's form is rendered exactly as a built-in's. A `forms_accept`
scenario drives it through the UI.

## Non-goals

- The rest of M3's accept: the frozen badge on bodies, the tree after a
  failed fillet as its own scenario, the gesture-count test over the whole
  flow, and M3's closing scenario: the next M3 plan.
- The tree's own edits: rollback bar, suppress, delete, reorder, rename
  in place. The form's name field is the only rename.
- Panels, context-menu entries, drawables and gizmos (`docs/UI-RENDERING.md`
  §Declarative plugin UI): no plugin needs one yet. No `ui` export in the
  world.
- A sketch's form: a sketch is edited in sketch mode, and created by
  `core.sketch.create` as now.
- A preview of an edit form: an edit commits field by field, so the
  document is the preview.
- A table of the document's parameters, a settings UI, toggle and free
  text widgets (no type declares one).
- A plugin's `migrate`, and any `type_version` bump beyond what a declared
  choice needs (none expected).

## Design deltas

- **ADR-0018, forms are the feature type's spec** (step 1): a form is
  derived from `FeatureTypeSpec` and nothing else, so a plugin's form
  needs no UI export; choices are declared (`ChoiceSpec`: name, title,
  options with titles, default); a parameter or input may be shown only
  under a choice's values (`shown_when`), which is how extrude's `target`
  appears in `join` and `cut` and the hole's counterbore fields under
  `counterbore`; a create form is a draft (session state) ending in one
  `AddFeature`, an edit form commits field by field (`EditFeature`).
- **ADR-0019, a preview is a speculative evaluation** (step 2): the
  session evaluates a snapshot with a command applied that is never
  applied to the document; it is session state, not a command, issues no
  generation, and is superseded by the next preview or cleared. Its
  outcome and meshes come as events tagged with the preview's own counter.
- `docs/ARCHITECTURE.md` §The protocol boundary: `Request::Preview {
  preview, envelope }` (the envelope carries the base) and
  `Request::ClearPreview`, answered `Outcome::Previewing`;
  `SessionEvent::previewed` and `preview_meshed` (its own variant, not a
  tag on `meshed`: step 2), round-tripped through JSON like every request.
- `arrix-plugin-api` **0.5.0, breaking** (a minor while 0.x): `ChoiceSpec`
  and `FeatureTypeSpec::choices`; `shown_when` on `ParamSpec` and
  `InputSpec`. The WIT world, the equality test, `plugins/gears`
  (`api = "^0.5"`).
- `docs/DATA-MODEL.md` §Features: the evaluator gives every declared
  choice to `FeatureArgs` (the record's or the default), refuses a choice
  the type does not list (`feature.unknown-field`) and a value it does
  not list (`feature.choice`); the per-type codes for a bad value
  (`extrude.mode`, `revolve.mode`, `boolean.op`, `hole.extent`,
  `hole.kind`, `sketch.world`) go if step 3 finds them unreachable.
  **No schema bump**: the record already stores choices.
- `docs/UI-RENDERING.md`: §The shell (the property card, its `auto`
  rule), §View types (`FormView`, its fields, the form intents), §The
  viewport (finished sketches drawn in part mode and pickable while a
  field asks; the preview's bodies), §Declarative plugin UI (forms from
  the spec, `shown_when`, validation as built), §Visual debugging (the
  form and the preview in `debug_state()`).
- `arrix-config`: `core.form.ok` (Enter) and `core.form.cancel` (Escape)
  in a new `form` keymap context, between `text` and `sketch-tool` in
  precedence; `docs/CONFIG-CATALOG.md` regenerated.
- New modules, not growth of `crates/arrix-app/src/lib.rs` (965 lines):
  `arrix-app` `form.rs`, `form_pick.rs`, `preview.rs`; `arrix-ui`
  `form.rs`; `arrix-doc` `session/preview.rs`.

## Arris dependencies

None new. Previews use the operations the features already call and
`Kernel::tessellate` (`arris` 0.5).

## Steps

- [x] **[1]** Step 1: ADR-0018 (forms are the feature type's spec), with
  the product scenario it serves; the ADR index.
- [x] **[3]** Step 2: previews in the session, headless, and ADR-0019
  (a preview is a speculative evaluation) written from what the step
  establishes, OPEN 4 included. `Request::Preview`
  evaluates the base snapshot with the command applied on the evaluator,
  memo cache shared (it is keyed by input hash), and publishes
  `previewed` and preview-tagged `meshed` events; a newer preview
  supersedes an older one's events, `ClearPreview` drops them, a preview
  against a stale base is rejected as a command is. Tests: an extrude
  previewed gives the meshes and outcome the same command gives once
  applied; the document's bytes and generation unchanged after any
  preview; a superseded preview's late events are dropped; requests and
  events round-trip through JSON.
- [x] **[2]** Step 3: plugin API 0.5, choices declared. `ChoiceSpec`,
  `shown_when`, the WIT world and `wit_equality.rs`; every built-in
  declares its choices and where its fields show; the evaluator fills
  defaults and checks names and values. Watch the input hash: filling a
  default must not change the hash of a record that already gave it, and
  the frozen goldens (`crates/arrix-cli/tests/golden/`) must stay byte
  equal, or the diff is read and the reason written in the commit.
  `gears` to `^0.5`; the backlog line is already gone.
- [ ] **[2]** Step 4: `FormView` and its renderer in `arrix-ui`: a header
  (type title, name field, state and diagnostic), quantity and count
  fields (expression text, unit hint, per-field message), choice
  combos, single and list reference fields (each value a row with a
  remove button, the active field marked), OK and Cancel on a create
  form. Intents `FieldCommitted`, `FieldEdited`, `ChoiceSelected`,
  `ReferenceFieldActivated`, `ReferenceRemoved`, `FormOk`,
  `FormCancelled`. A kittest test renders a form of every field kind
  headless; `arrix-ui` names no doc type (the layer lint).
- [ ] **[2]** Step 5: the create form. `<type>.create` opens the property
  card with a draft from the spec and `ArrixApp::effective_params`;
  a field commit is checked by `Expr::quantity` against the replica's
  parameters and the spec's kind, a refusal keeping the text with the
  reason; `shown_when` hides fields; OK is one `AddFeature` (a `Group`
  with `AddPart` in a document with no part), at the end of the part as
  `core.sketch.create` places a sketch; Cancel and Escape issue none. The
  `form` context, `core.form.ok` and `core.form.cancel`, the catalog. A
  table test creates every type with a `.create` command, gears
  included, each one command; the gesture count is 1 for OK and 0 for
  everything before it.
- [ ] **[2]** Step 6: the edit form. Double-clicking a non-sketch feature
  in the tree (`TreeFeatureActivated`) opens its form from the record;
  a field commit or a choice is one `EditFeature`, an equal value none,
  a refused text none; the form is rebuilt from the replica each frame,
  so an undo or redo with it open shows the document's values
  (principle 4); deleting the feature by undo closes it.
- [ ] **[2]** Step 7: references picked in the scene and the tree. The
  active reference field filters what a pick means: a planar face to a
  `plane` (`Ref::Topo`), a face of a body to its `body` slot
  (`Ref::Slot` of the slot the scene keys it by), an edge to an `edge`, a
  vertex to a `point`, a straight edge to an `axis`; in the tree a datum
  plane to a `plane`, a feature to a `tool` (`Ref::Feature`), a body's
  maker to a `body`. A pick that fits nothing says what the field takes
  in the status line and issues nothing. Shift-click adds to or removes
  from a list field. The selection when the create command runs seeds
  the first field it fits. The field's references are drawn selected in
  the viewport. Edit-form picks are each one `EditFeature`.
- [ ] **[2]** Step 8: finished sketches in part mode. Each visible sketch
  is drawn on its plane through `PlaneToScreen` (curves and points, no
  dimensions or glyphs), and while a field asks for a `region`, `point`
  or `axis` its regions, points and lines are hit-tested as sketch mode
  does and picked to `Ref::Region` (the key the evaluator gave) or
  `Ref::Sketch`; the hovered region is shaded. The scene's pick and the
  sketches' hit test are ranked so a sketch item over a face wins while
  such a field is active, and never otherwise. Scratch render read
  before the snapshot.
- [ ] **[3]** Step 9: the preview in the app. Every create-form change
  that leaves the draft complete enough to evaluate sends a
  `Request::Preview`; the viewport draws the previewed part in place of
  the current one (bodies a preview replaces hidden, new or changed ones
  in a preview tint) and the form's header shows the preview's
  diagnostic; Cancel and OK clear it. An applied change ends the
  session's preview (ADR-0019), so a draft still open previews again
  against the new generation; `PreviewBody::changed` says which bodies to
  tint, and a generation body missing from the list is hidden. The preview issues no command
  (gesture count 0 per field) and survives a slow evaluation without
  blocking the UI. `debug_state()` reports the preview's state and
  bodies. A scenario of an extrude's preview and its committed result
  with the same counts.
- [ ] **[2]** Step 10: diagnostics on the form. A `ref.lost` input shows
  lost in its field, its candidates highlighted in the viewport, and a
  pick of one re-binds it (one `EditFeature`, never automatic); a failed
  feature's diagnostic heads its form and its referenced entities are
  highlighted; a frozen or frozen-stale feature opens read only, its
  stored values by name when its type is absent, with the reason.
- [ ] **[2]** Step 11: the `forms_accept` scenario (below), its golden
  written with `UPDATE_SNAPSHOTS=1` after the scratch and baseline
  images were read, and `docs/UI-RENDERING.md` brought up to the code
  in the same commit.

## Acceptance

`cargo test -p arrix-app --test visual forms_accept`, driven through the
UI alone on a document whose sketch is drawn in sketch mode:

1. Part tab, New extrude: the region picked in the viewport, the
   distance typed `12 mm`; the preview shows the plate before OK; OK is
   one command, and the body's counts match the preview's.
2. The extrude edited from the tree: distance `w` (a document parameter)
   is one command; `12 deg` is refused in the field with
   `expr.unit-mismatch` and issues none; mode `cut` shows `target`.
3. Gears tab, New spur gear on a datum plane picked in the tree, teeth
   `24`: one command; its form, reopened, shows the stored values.
4. New fillet on an edge of the plate with radius `50 mm`: the tree and
   the form show `kernel.degenerate.blend-too-large`; the radius edited
   to `1 mm` evaluates.
5. The plate's sketch redrawn so the filleted edge goes: the fillet's
   edge field shows lost with candidates; picking one is one command and
   the fillet evaluates.
6. The gesture count after every gesture of the flow is 1 or 0 as
   stated, the golden holds the form views and the frames, and the whole
   gate is green, the wasm build included.

## Docs to update on completion

- `docs/UI-RENDERING.md` §The shell (property card), §View types
  (`FormView`, form intents, "the property card is empty until forms
  exist" removed), §Declarative plugin UI (forms from the spec,
  `shown_when`, validation as built), §The viewport (sketches in part
  mode, preview bodies, Overlays), §Visual debugging (form and preview in
  `debug_state()`).
- `docs/PLUGINS.md` §One interface (world 0.5.0, choices, `shown_when`),
  §Tier 0 as built ("a plugin type takes no choices" removed),
  §Versioning (0.5.0's line).
- `docs/DATA-MODEL.md` §Features (declared choices, defaults, codes),
  §The core feature types (each type's choices as declared; retired
  codes).
- `docs/ARCHITECTURE.md` §The protocol boundary (preview request and
  events).
- `docs/CONCURRENCY-WASM.md` (a preview on the evaluator: superseded,
  never blocking a commit's evaluation), if step 2 decides an ordering.
- `docs/CONFIG-CATALOG.md` regenerated; `docs/BACKLOG.md` (the choices
  line removed with this plan; new lines for what steps defer).
- `docs/ROADMAP.md` C1 status line: M3 plan 4 (`forms`) landed.
- `AGENTS.md` current state: M3 plans 1 to 4, the next M3 plan named.

## Open questions

- Decided 2026-10-08 (human): a create form is a draft committed by OK
  as one command, Cancel leaving nothing in undo (ADR-0018); the preview
  is in this plan (steps 2 and 9, ADR-0019).
- Decided 2026-10-08 at step 3 (OPEN 3): a bad choice value is checked
  once, by the evaluator (`feature.choice`); the per-type codes
  (`extrude.mode`, `revolve.mode`, `boolean.op`, `hole.extent`,
  `hole.kind`, `sketch.world`) are retired, and a name no choice declares
  is `feature.unknown-field` (it was `feature.unknown-choice`). No golden
  held either. A choice with no default (`ChoiceSpec::default` is an
  `Option`) stays unset, which a datum plane's and a sketch's `world`
  needs: unset, they stand on their `plane` input. `evaluate` gained a
  `choices` argument, which the plan's list left out and ADR-0018 asks
  for. The input hash covers the record's choices as written.
- Decided 2026-10-08 at step 2 (OPEN 4, ADR-0019 decision 4): a preview
  waits behind the newest commit's evaluation and meshing and never stops
  it; a commit, a newer preview or a clear stops a running one, and an
  applied change ends it.
