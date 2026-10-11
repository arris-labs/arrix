# UI and rendering

The egui shell, the ribbon and palette, declarative plugin UI, sketch mode,
the viewport and picking, and how the agent sees all of it headless. Crates:
`arrix-ui` (widgets over view types), `arrix-viewport` (wgpu), `arrix-app`
(the shell and the translation to and from the document). The stack is
egui/eframe/wgpu (`SEED.md` §9), pinned to one egui minor for the workspace.
Built, as of C1: the shell (ADR-0017), the viewport and picking, sketch
mode, forms and previews, the configurable shell (ADR-0015) and the
headless harness. Where a section says "not built", it is later than C1.

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
│ ribbon: Sketch │ Part │ <plugin tabs> …                              │
├──────────────────────────────────────────────────────────────────────┤
│ ┌─────────────┐                                      ┌─────────────┐ │
│ │ feature tree│              viewport      ViewCube  │ properties  │ │
│ │ (card)      │                                      │ (card)      │ │
│ └─────────────┘                                      └─────────────┘ │
├──────────────────────────────────────────────────────────────────────┤
│ document tabs                                                        │
├──────────────────────────────────────────────────────────────────────┤
│ status: help for what is under the pointer │ generation, diagnostics, │
│                                            │ units                    │
└──────────────────────────────────────────────────────────────────────┘
```

Two bars are docked (ADR-0017): the ribbon on top, and at the bottom the tab
strip of the open documents over the status line. The viewport fills what is
between them. The feature tree and the property panel are **cards**, areas
over the viewport that exist only while they show: `ui.panel.tree` and
`ui.panel.properties` are `auto`, `shown` or `hidden`, and the
`core.panel.*.toggle` commands flip a card for the session over the setting.
`auto` is the tree once the document has a part, and the property card while a
form is open. The status line's left side is help text: the title, keys
and what it needs of the ribbon button or palette row under the pointer,
else what to do next.

- **The command registry** holds every user action as a `CommandSpec`: id
  (`core.palette.open`, `gears.spur.create`), title, ribbon placement,
  default keys, the keymap context they apply in, and whether it needs an
  open document. The data is in `arrix-config`; `arrix-app` owns whether a
  command applies in the current session state, and the dispatch. The
  ribbon, the command palette, the context menus and the shortcut map all
  read it. One `<type>.create` exists per registered feature type, the
  plugins' included, and each tab has a `core.ribbon.<tab>` command. A
  creation command is disabled while no document is open. **Installing a
  plugin adds a ribbon tab, never a mode** (`SEED.md` §5).
  `ui.ribbon.hidden-tabs` hides any tab, core ones too; the palette still
  reaches every command.
- **The keymap** is each command's default keys with the user's
  `keybindings.toml` over them: `[[bind]] key = "ctrl+k"`, `command =
  "…"`, optional `when`. Keys are modifiers (`ctrl`, `shift`, `alt`,
  `super`) and a letter, digit, `f1` to `f12` or named key. A user binding
  replaces its command's defaults and an empty `key = []` unbinds it. A
  bad entry is one diagnostic with file and line and is left out, so the
  rest still applies. `when` is one context name: `part`, `sketch`,
  `form`, `sketch-tool`, `palette` or `text`; a binding naming an active context
  beats a global one, of two active contexts the more specific wins
  (`text`, then `form`, then `sketch-tool` over `sketch`: Escape cancels
  an open form before it ends a drawing, and ends a drawing before it
  finishes the sketch), and while a text field has focus only `text`
  bindings fire. `arrix-app`
  turns each egui key press (not its repeats) into a chord, resolves it,
  and runs the command the way a ribbon button does. `debug_state()`
  reports the keymap in force and the config diagnostics.
- **The command palette** (`Ctrl+K`, `core.palette.open`) searches every
  listed command by title and id: every word of the query must match, and
  titles that start with the first word come first. Each result shows its
  effective keys and is dimmed when it doesn't apply (a creation command
  with no document open); Enter on a dimmed one does nothing. It is
  session state and issues no document command; running a result closes
  it. While it is open it owns the keyboard: the `palette` and `text`
  contexts are active, and its own keys are commands bound in `palette`
  (`core.palette.close` on Escape and Ctrl+K, `.next` and `.previous` on
  Down and Up, `.run` on Enter), so they are rebindable like any other. A
  key that ran a command is removed from the frame's input, so the query
  field doesn't also act on it. `debug_state()` reports the query, the
  results and the selection in the shell view.
- **Undo, redo and new document** are the UI's first document commands.
  `core.edit.undo` (Ctrl+Z) and `core.edit.redo` (Ctrl+Shift+Z, Ctrl+Y) ask
  the tab's session to undo or redo the local author's last change; a
  stack that is empty is a notice in the status line, never silence.
  `core.document.new` (Ctrl+N) opens an empty document in a new tab, with
  no part until something needs one. `core.file.export` (palette only) asks
  a save dialog (`crates/arrix-app/src/file_dialog`, behind a trait so a
  scenario answers it) for a path, takes the format from its extension
  (`.step`, `.stl`, `.obj`; any other is a notice; a `.step` comes back
  refused as `kernel.export.unit`, ARCHITECTURE §The kernel choke point),
  and sends the session
  `Request::Export` for the bodies the selection is in, every body with
  none selected; the file is written when the `Exported` event arrives, and
  the status line says so. It is a request, not a command: its gesture
  counts zero. Each key or intent the app handles
  (a hover is none) is a **gesture**, opened and closed around its
  handling (a drag is one, closed by its release; a click on a finished
  sketch's item while a field asks for one is one too); the commands it
  got applied to the document it began on are counted. The app keeps a
  **gesture log**, session state: each closed gesture, what opened it
  (`key <command>`, `command <command>` for a button or palette row, an
  intent's kind, `submit`) and its count. `debug_state()` reports the last
  gesture's count (`gesture_commands`), the log (`gestures`, the last 256)
  and the active document's applied total (`commands_applied`), which is
  how a scenario checks over a whole flow that every gesture was one
  command, or none where nothing changed, and that the log's sum is the
  total, so no command was applied outside a gesture. Such a command is a
  bug, not a case the log records: a debug assertion at each gesture's
  close catches it.
- **Themes** are `ThemeView` data (below), made from a theme file: `base`
  (`dark` or `light`, the toolkit's look for what the roles don't cover),
  `[vars]` and `[colors]` over 16 roles (`THEME_ROLES`). A colour is `#rgb`,
  `#rrggbb` or `#rrggbbaa`, and a var is referenced as `"$name"`. `dark` and
  `light` are built in, in the same format; `ui.theme` names one of them or
  `themes/<name>.toml`. A missing role takes the base's and is one warning
  naming all of them. A theme never touches the text cursor's blink, which
  the test harness turns off for determinism.
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
  with its candidates highlighted, and listed in its form (below).
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
  and what a thin client would receive. The scene is the viewport's own
  (`arrix_viewport`, fed `RenderMesh`es), and the camera is session state
  that `debug_state()` reports; neither is a view type of `arrix-ui`. Built so far: `ShellView`, holding
  a `RibbonView` (tabs, the open one, and its groups of buttons with
  whether each is enabled, and a tool's whether it is the one in use), a `PaletteView` (the query, results and
  selection, while open), a `TreeView` (each part's features with their `FeatureState`), a `DocumentTabsView`
  (the open documents), a `CardsView` (which cards show), a
  `StatusView` (the help text, generation, diagnostics count, display units,
  config diagnostics included) and a `ThemeView` (the 16 roles as colours, and the
  dark or light base), which `arrix-ui` turns into egui `Visuals`; `arrix-ui`
  does not depend on `arrix-config`. The property card holds the shell's
  `FormView` while a form is open (below), and says "Nothing is being
  edited" otherwise.
- **`BadgeView`** (`arrix_ui::badge`) is a frozen badge as the app placed
  it: the feature's id and name, `Frozen` or `FrozenStale`, the reason it
  is frozen, and the chip's centre in points. `arrix-ui` paints the chips
  and says which one the pointer is over.
- **`FormView`** (`arrix_ui::form`, ADR-0018) is a feature type's form as
  data: an id naming the form being edited (a different id starts every
  text buffer afresh), the type's title, whether it is a create form (OK
  and Cancel) or an edit form, the feature's name, its `FeatureState` and
  diagnostic (code and message), a read-only reason, and the fields the
  spec's `shown_when` leaves shown, in the spec's order. A `FormField` is
  a name (the spec's), a title, a message (why its text was refused) and a
  `FieldValue`: `Quantity { text, unit }` (an expression; the unit names
  what the text is measured in with none; a count has none), `Choice {
  selected, options, clearable }` (a combo; `clearable` where the choice
  has no default), or `Reference { takes, values, list, active,
  candidates }` (each value a row with a remove button and a lost one in
  red; the active field is the one a pick fills; `candidates` are a lost
  value's re-pick rows, best first). One renderer, `arrix_ui::form`, draws every
  type's form, so a plugin's is a built-in's. A text box's buffer is the
  renderer's, kept in egui memory by form id and field name: it follows
  the view's text when that changes (an undo with the form open) and keeps
  what was typed when it does not (a refused text stays with its reason);
  Enter or leaving the box commits, Escape puts the view's text back.
- **The shell's layout** is a function of the window size and of which cards
  show: the ribbon 88 points high, the tab strip 24, the status line 20, and
  a card at a fixed width (the tree 240, the properties 280) pinned 8 points
  inside the viewport's top-left or top-right corner; cards are not draggable.
  `arrix_ui::shell` returns where each region landed (`None` for a card not
  showing), which `debug_state()` reports.
- **Intents** are what a user did, in view terms: the form's `FieldEdited
  { field, text }` (uncommitted), `FieldCommitted { field, text }`,
  `NameCommitted { text }`, `ChoiceSelected { field, value }` (`None`
  clears a choice that may be unset), `ReferenceFieldActivated { field }`,
  `ReferenceRemoved { field, index }`, `CandidatePicked { field, index }`,
  `FormOk` and `FormCancelled`;
  `CommandInvoked { command }` (a ribbon button, a palette row or a key),
  `RibbonTabSelected { tab }`, `PaletteQuery { text }`, `PaletteClosed`,
  `DocumentTabSelected { index }`, `DocumentTabClosed { index }`,
  `Picked { name }`, `Hovered { name }`, `TreeFeatureSelected { feature }`,
  `TreeFeatureActivated { feature }`, `SketchHovered { hit }`,
  `SketchClicked { hit, extend, pointer }`, `SketchPointerMoved { pointer }`
  (a `PlanePointer`: plane-local metres, the snap radius in metres, whether
  Alt is held, and egui's clock). The ViewCube returns a
  `ViewCubeAction` of its own, which the app applies to its camera.
  `arrix-app` turns an intent into a `CommandEnvelope`, or into a change of
  session state (selection, camera), which issues no command.

## Configuration

The shell is data and the data is text (ADR-0015): a user, or their agent,
edits TOML in the config directory and sees the change without restarting.
Config is client state. It never enters the document and never reaches a
feature's `evaluate`.

- **Where**: `$ARRIX_CONFIG_DIR`, else `arrix` under `$XDG_CONFIG_HOME` or
  `~/.config` (Linux), `~/Library/Application Support` (macOS) or
  `%APPDATA%` (Windows). `arrix_config::config_dir` is a pure function over
  an environment its caller hands in, shared by the app and the CLI. No
  directory is the built-in configuration.
- **The files**: `settings.toml` (`[ui]` and `[defaults."<feature-type>"]`),
  `keybindings.toml` (`[[bind]]`, §The shell) and `themes/<name>.toml`.
  Two layers, the built-in defaults and then these files. The app never
  writes them.
- **Fail soft**: a key that is unknown, of the wrong type or lost to a
  syntax error keeps its default and is one diagnostic with its file, line
  and code; the rest of the file still applies. A theme or binding that can't
  be had falls back the same way. Diagnostics show in the status bar count
  and in `debug_state()`.
- **Parameter defaults**: `[defaults."gears.spur"] module = "1.5 mm"` is
  checked against the type's `ParamSpec` (unknown type or parameter, an
  expression that doesn't parse, the wrong quantity kind) by `arrix-doc`'s
  parser, which is handed to `arrix-config`. `ArrixApp::effective_params`
  gives a new feature's initial form. Once a feature is committed its
  values are the document's, so a default never reaches `evaluate`; a test
  evaluates a document under two configs to identical JSON.
- **Reload**: `core.config.reload` re-reads the files and applies them. A
  source that can fingerprint its files (`ConfigSource::fingerprint`; the
  directory hashes names, sizes and modification times) is polled about once
  a second of egui's clock, and a change reloads. Only `config_files` in
  `arrix-app` touches the disk; the browser gets a store behind the same
  seam in C4.
- **For agents**: `arrix describe [--json]` lists everything there is to
  configure and its effective value (`"schema": 1`); `arrix config check`
  prints one JSON line per diagnostic and exits 1 if there is any;
  `arrix config schema` gives JSON Schemas of the three file kinds for
  taplo; `docs/CONFIG-CATALOG.md` is the generated reference, held fresh by
  a test (`UPDATE_SNAPSHOTS=1 cargo test -p arrix-cli catalog`).

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

**Forms as built** (ADR-0018). A form is derived from the feature type's
`FeatureTypeSpec` and nothing else, so a plugin's is a built-in's: the
spec's choices (a combo each; one with no default may be cleared), then its
inputs (a reference field each), then its parameters (an expression, with
the unit it is measured in when it names none), each in the spec's order and
each only while its `shown_when` choice holds one of the listed values.
`<type>.create` (every type but `core.sketch`, which is made and edited in
sketch mode) opens a **create form** in the property card: a draft on the
document's tab, session state, seeded with the type's spec and the user's
parameter defaults (`ArrixApp::effective_params`) and named with the first
free `<Title> <n>` of the part. A field commit is checked by the form's own
rules, `Expr::quantity` of the field's kind against the document's
parameters as the replica last heard them; a refusal keeps the text in its
field with the expression's code and reason beneath it, and issues nothing.
OK is one `AddFeature` at the end of the part (a `Group` with `AddPart` in a
document with no part, as `core.sketch.create` places a sketch), selects the
new feature in the tree and closes the card's form; OK with a refused field
open says why in the status line and adds nothing. Cancel and Escape drop
the draft and issue none; Enter is OK while no field has focus
(`core.form.ok` and `core.form.cancel`, bound in the `form` context). A
form does not open in sketch mode (the status line says to finish the
sketch). A create form opens with its first reference field active.

**References are picked.** The active reference field decides what a pick
means (`form_pick.rs`): in the viewport a face is a `plane` (`Ref::Topo`)
or, for a `body` field, the body it is on (`Ref::Slot` of the key the scene
holds it by), an edge is an `edge` or an `axis` (whether it is straight is
the evaluator's to say), a vertex a `point`; in the tree a datum plane is a
`plane` (`Ref::Slot`), any feature a `tool` (`Ref::Feature`) and a feature
that makes a body a `body`. A pick that fits nothing says in the status line
what the field takes, changes nothing and selects nothing, and while a field
is active a click in the scene or the tree is a pick, never a selection.
A list field takes shift-click to add a reference or, when it holds it,
remove it; without shift a pick replaces the list. Each value is a row with
a remove button. A create form's picks change the draft; an edit form's
each is one `EditFeature`, an equal value none. The selection when
`<type>.create` runs seeds the form: the scene's, then the tree's, each into
the first empty field it fits. What the fields hold in the scene is drawn as
the selection is (`Highlights::marked`).

An **edit form** is the same renderer over an existing feature: a double
click on a tree row (`TreeFeatureActivated`) of any feature that is neither
a sketch nor a datum plane opens it from the record, with no command. It is
rebuilt from the replica every frame, so an undo or redo with it open shows
the document's values, and undoing the feature's creation closes it. It has
no OK or Cancel: a field commit is one `EditFeature`, a choice is one, the
name field is one; a text equal to the record's, or one the form's rules
refuse (kept in its field with the reason), issues none. Enter and Escape
only close it. A typed text the record does not say yet is kept until it
does, so a committed field does not flicker back to the old value.

An edit form heads with where the feature stands and what evaluation said
of it (`standing_of`, the tree's own answer). A failed feature's diagnostic
shows under the title, and the entities it is about are marked in the scene
as a selection is. A `ref.lost` diagnostic marks the field that holds a lost
reference: the value in red, a message, the field made the active one when
the loss is first seen, and the diagnostic's candidates marked in the
scene, region and sketch ones in the finished sketches. A failed modifier
leaves its part with no body to click (ADR-0007: the reader is unavailable
rather than given an earlier version), so the form also lists the first
eight candidates as rows, best first; taking one, or picking any fitting
entity into the field, is one `EditFeature` that replaces the lost
reference in place, and never happens by itself. A frozen or frozen-stale
feature opens read only: its fields are disabled, the reason says which
type or version is missing, and a feature whose type is absent shows the
names and values its record stores. A read-only form takes no edit
intent.

Tier 0 drawing its own egui is an escape hatch decided by ADR when a real
plugin needs it, not before.

## Sketch mode

Sketch mode edits one `core.sketch` feature. It is session state, one
sketch per document tab; opening a sketch issues no command, and the camera
turns to face the plane in orthographic projection.

- **Entering** is `core.sketch.create`, which is one command (an
  `AddFeature`, in a document with no part a `Group` of `AddPart` and
  `AddFeature`), or `core.sketch.edit`, which is none. Create stands the
  sketch on the selected planar face (`Ref::Topo`), else on the datum plane
  selected in the tree (`Ref::Slot`), else on the world XY plane; it lands
  at the end of the part the face or datum plane is in, else of the first.
  A face that is not planar is not checked for here: the feature fails
  with its diagnostic, in the tree. A tree row is clicked to select it
  (`TreeFeatureSelected`) and double-clicked to act on it
  (`TreeFeatureActivated`): a sketch opens, a datum plane gets a sketch.
  `core.sketch.edit` opens the sketch selected in the tree.
- **Finishing** is `core.sketch.finish` (Escape, in the `sketch` context),
  which issues no command. Undoing the creation of the open sketch leaves
  sketch mode, since there is nothing left to edit.
- **The camera** turns once the sketch's plane is known, which is when its
  evaluation arrives: the plane's normal gives the turntable's yaw and
  pitch (a plane facing Z keeps the yaw it has), the target is the plane's
  origin and the view is orthographic, fitted to 100 mm or to the bodies.
  Finishing flies it back to the pose, projection and depth range it had.
  The turntable keeps Z up, so a tilted plane's axes are not on the screen's.
  In sketch mode the `sketch` context is active in place of `part`, so the
  view keys are off, and `sketch-tool` with it while a drawing tool is in
  use.

- **The overlay is a `SketchView`**, built by the app from the sketch as
  last evaluated and solved again (`Diagnostics::evaluate`, a draft made
  anew when a newer evaluation of the sketch arrives): its points and
  curves (a line, a circle, or an arc counter-clockwise from its start) in
  plane-local metres, each with its constraint state (under-constrained,
  fully, fixed, conflicting), construction curves, a mark beside each
  entity a geometric constraint holds, each dimension's label (the value in
  mm or deg, with its expression in front when it has one) and leader, and
  the DoF and the solver state (under-constrained, fully constrained,
  over-constrained, did not converge) that the status line shows.
  `arrix-ui` paints it through a `PlaneToScreen` the app hands it, a
  homography built from the camera and the plane's frame, so it is exact
  in both projections and inverts (a screen point to the plane, for
  picking). Leaders stand 6 mm off what they measure, away from the middle
  of the sketch, and have no extension lines yet. The view is in
  `debug_state()` (`sketch_view`), which is how a scenario reads it.
  The camera fits the sketch's own points, a hundred millimetres for an
  empty one.
- **Hit testing, hover and selection** are in the plane, on the screen's
  scale: a pointer within 7 points of an item hits it, and where several
  are under it a point beats a curve, a curve a constraint's mark and a
  mark a dimension (its label's box or its leader), the nearest of a kind
  winning and the first in the view a tie, so coincident items answer the
  same every time. Hovering is amber and names the item in the status line;
  a click selects it and shift-click adds it or takes it away, a click on
  empty space clears the selection. The selection is session state, any
  record of the sketch by id, and what an edit or an undo removes leaves it.
  In sketch mode the pointer is the sketch's: nothing in the scene is picked
  or hovered. `core.sketch.delete` (Delete) removes the selection and what
  stands on it (`SketchEdit::remove`) as one command, and none when there
  is nothing to remove.
- **The tools** are in the Sketch tab's Draw group: `core.sketch.select`,
  `core.sketch.line` (L), `core.sketch.rectangle` (R),
  `core.sketch.circle` (C) and `core.sketch.arc` (A), the one in use
  drawn pressed. A drawing tool's pointer highlights nothing and selects
  nothing: every frame it is over the viewport its snap is searched over
  the solved draft and drawn as the `SketchView`'s `preview` (the cursor
  where it snapped, what the next click draws, the glyphs with the
  constraint kinds each commits, the alignment guides, the acquired
  points), and the status line says what the next click does. The line's
  first click places its start and issues nothing; each later click draws
  a segment from the last end, one `SketchEdit`, and the chain goes on
  until a click on its end (the second click of a double-click), a segment
  closed on its first point, or Escape (`core.sketch.tool.cancel`, in
  `sketch-tool`). With nothing pending Escape returns to the select tool,
  and with the select tool it finishes the sketch. The rectangle is two
  clicks, the first issuing nothing: four lines on four shared corners
  (the corners are coincident by sharing their points, as the line's
  chain does), the two level sides `Horizontal` and the others
  `Vertical`, then the first corner's inferences and the opposite
  corner's, by the commit rule; a click level or plumb with the first
  corner draws nothing. The circle is two clicks, the centre issuing
  nothing: its centre's inferences are bound, and a radius click snaps
  to a point only (what it binds is that the rim passes through it,
  `PointOnCircle`, so the glyph says that and no other snap is shown
  there); a radius within the snap tolerance draws nothing. The arc is
  three clicks, the first two its ends, with their inferences, and the
  third a point it passes through, a plain place with no snap: the arc
  is the circle through the three, counter-clockwise from whichever end
  it runs from, its centre a free point; three points on a line draw
  nothing. A line from outside a circle or arc that ends at the tangent
  snap commits `Tangent` alone (the end is placed on the rim, not bound
  to it). The click's edit is the diff between the solved
  draft before it and after it, solved again (both carry the resolved
  values of the sketch's expressions, so only what the click made rides
  along), and the draft it leaves is drawn until the evaluation of the
  edit arrives, so the next click draws on it. A draft that does not
  solve draws nothing and says so.
- **Constraints from the selection** are one command per kind,
  `core.sketch.constrain.<kind>` (the Constrain and Place groups of the
  Sketch tab; the group row scrolls sideways where the window is
  narrower): coincident, horizontal and vertical (on each selected line),
  parallel, perpendicular, equal (two lines, or two circles or arcs),
  concentric, tangent (a line and a circle or arc, or two of those),
  symmetric (two points about a line, or three points, the one nearest the
  others' midpoint the centre), point on (a line, circle or arc), midpoint,
  align horizontally and vertically (two points), fix, and a point on the
  origin or on either axis. Each is enabled only for a selection that fits
  it, and the status line says what it needs where it is not. Each
  constraint is checked with `check_candidate` before it goes in: a
  redundant or conflicting one is refused with the reason in the status
  line and no command, and of several (horizontal on three lines) the ones
  that hold are added and the rest dropped. The draft is solved so the
  geometry meets what was added, and the change, the moved points
  included, is one `SketchEdit`. `core.sketch.construction` (Q) makes the
  selected curves construction, or solid when every one already is. The
  dimension kinds and the datum distance, angle and symmetry are the
  dimension tool's, or are gated (`docs/DATA-MODEL.md` §Sketches).
- **Dimensions** are the dimension tool's (`core.sketch.dimension`, D)
  and a label's field. The tool picks points and curves (the picks are
  drawn selected): a line is its length, a circle or arc a radius (shift
  at the placing click, a diameter), two points a horizontal distance when
  the label is placed above or below them and a vertical one beside, a
  point and a line the distance to it, two lines the distance between them
  when parallel and the angle otherwise. A click that is not a pick the
  picks can grow by places the label there: one `SketchEdit` holding the
  dimension at the value the geometry measures. One the sketch already
  holds (the candidate check says redundant) is placed as a reference
  dimension, measured and not driving, and the status line says so.
  A double-click on a driving dimension's label opens its field over it,
  the text selected. The text is an expression of the dimension's
  quantity: it may name a parameter of the document, resolved from the
  replica's last finished values, and a number takes its unit
  (`45 mm`). Enter or losing focus commits: the draft is solved with the
  value, and the change, the constraint (its expression kept when it names
  a parameter) and the positions the solve gave, is one `SketchEdit`. A
  text that does not parse, names nothing, has the wrong unit, or a solve
  that fails leaves the field open with the reason and issues nothing;
  the value the dimension already has issues nothing; Escape dismisses it.
  A focused field owns the keyboard: no sketch key fires while one is
  open. When a parameter changes in the document the sketch is evaluated
  again and the label follows, with no command from sketch mode.
- **Trim and extend** are the modify tools (`core.sketch.trim` T,
  `core.sketch.extend` E; the Modify group). The pointer picks the drawn
  line, circle or arc under it (construction geometry is never picked) and
  the overlay previews the stretch the click acts on: for trim the piece
  between the crossings nearest the pointer, for extend the run from the
  end nearest the pointer to the first curve along its continuation. The
  preview and the edit read the same pieces and the same first hit
  (`arrix_sketch::modify`), so the click does what the hover showed. A
  click that changes the sketch is one `SketchEdit` of the draft solved
  after it, constraints transferred by the modify rules; a click on
  nothing issues nothing, and one the edit refuses (no crossing to reach)
  says why in the status line and issues nothing.
- **Offset and mirror** are the other two modify tools
  (`core.sketch.offset` O, `core.sketch.mirror` M). Offset: a click on a
  line, arc or circle opens a distance field at the click (the field of a
  dimension, `1 mm` to start); the text is an expression of a length, a
  parameter of the document may be named, and a negative distance is the
  other side. The copy of the chain the click belongs to is previewed as it
  is typed (`arrix_sketch::modify::offset_preview`, the geometry the edit
  writes); Enter or losing focus commits one `SketchEdit`, its distance rows
  keeping the expression when it names a parameter. A text that is no
  length, a distance the chain cannot take or a solve that fails leaves the
  field open with the reason and issues nothing. Mirror works on the
  selection, made with the select tool (the command needs a curve selected):
  hovering a line, a construction one too, previews the copies about it, and
  a click commits them as one `SketchEdit` and returns to the select tool.
  Hovered or clicked, anything but a line says to click one and issues
  nothing.
- **The drag** is the select tool's: the left button down on a point or a
  curve and moved starts a `DragSession` over the solved draft (a point
  that is fixed, a glyph and a dimension are not dragged; a curve is
  grabbed by its body, a circle by its rim). Each frame steps it to the
  cursor and the overlay draws the draft as the solver left it, with the
  diagnostics of before the press until the release; `debug_state()`
  reports the frames stepped and the last solve's microseconds while the
  button is down. A frame the solver cannot follow (it did not converge, or
  the branch guard refused it) leaves the geometry where it was and the
  status line says the point or curve cannot go further. The release is
  one `SketchEdit`, the diff between the draft before the press and the
  one the drag left, and none when nothing moved (a click without movement
  is a click, and selects). The press and the frames are not gestures; the
  release is.
- **The solver runs on the UI thread**, the one sanctioned exception to
  "the kernel never runs on the UI thread" (`SEED.md` §6.1): sketches are
  small, and a drag must follow the cursor in the same frame. It has a
  budget per frame (`docs/CONCURRENCY-WASM.md` §Budgets); a solve that
  does not converge within it leaves the geometry where it was and says
  so.
- **The solver** is `arrix-sketch`'s, ported (`SEED.md` §8.1) and
  headless: DogLeg by default and Levenberg–Marquardt selectable
  (`SolverAlgorithm`), an analytic sparse Jacobian, independent
  subsystems solved apart, DoF, redundancy and conflict from one rank
  analysis (`Diagnostics`), and validation of degenerate geometry and
  open gaps. Drag steps within the constraints' null space and a branch
  guard refuses a step that would flip an angle, a tangency side or an
  arc's sweep. Trim, extend, offset and mirror are pure edits of the
  model that transfer its constraints, each row they add kept only if
  the insert-time check (`Sketch::check_candidate`) passes, with hover
  previews from the same pieces. The same sketch solved twice gives
  bit-identical positions. The drag is inside its frame budget on a
  drawn part's sketch, and over it on one of mostly nonlinear rows
  (`docs/CONCURRENCY-WASM.md` §Budgets).
- **Inference is one engine** in `arrix-sketch` (`snap_search`), ported
  and headless: a search over candidate snaps in priority order (a point,
  the origin, a midpoint, an intersection, a tangency, a quadrant, on a
  curve or an acquired line's extension; then alignment guides with
  acquired points, the pending segment's H/V lock or a direction relation
  to a line, the sketch axes), in plane-local metres with a tolerance in
  pixels turned metres. Its result is drawn as glyphs and guides, and the
  click commits the same inferences by the commit rule: bound one at a
  time in priority order (an object snap, the line's own orientation, a
  relation, an alignment, an axis), each checked with
  `Sketch::check_candidate` against the sketch with the earlier ones in;
  a redundant one is drawn but not committed, a conflicting one neither.
  A tangent snap commits `Tangent` alone: with the end held on the rim
  too, the two are one first-order row at the touching point
  (`crates/arrix-sketch/tests/inference_probe.rs`). Alignment sources are
  the origin, the line chain's own points, and the points and lines the
  pointer rests on for 0.3 s of egui's clock (a dwell; a few are kept, the
  oldest let go first, and a change of tool lets them all go); holding Alt
  suppresses inference. The snap radius is the hit tolerance, 7 points, in
  metres where the pointer is. The sketch fillet,
  auto-constrain and the constraint descriptions are not ported yet.
- **A sketch edit is one command.** Drawing a line with its inferred
  coincidence and horizontal is one `SketchEdit`; a drag is one on release;
  trim, extend, offset and mirror are each one.
- **Regions** are shaded from the sketch's planar arrangement, and the
  region a profile pick stores is the one shaded (`docs/DATA-MODEL.md`
  §Sketches). The app keeps the draft's faces beside it
  (`arrix_sketch::find_region_outlines`, the evaluator's own call, so a
  draft equal to the saved sketch has the evaluator's regions, keys
  included) and recomputes them when the draft changes, a drag frame
  included. `SketchView.regions` carries each region's key (the JSON
  `Ref::Region` stores), its hole count and, off the debug state, its
  boundary in plane-local metres; the viewport fills it under the curves
  as a band-by-band trapezoid mesh (egui's path fill is convex-only and
  has no holes), so a plate with a bore is shaded around the bore and the
  disc in the bore is a region of its own. Construction curves bound
  nothing, and an open curve shades nothing. With the select tool and
  nothing else under it the pointer's innermost region is
  `hover_region`, outlined.
- **Constraint kinds reachable from the UI are the only ones a document
  holds.** Enabled: coincident, horizontal, vertical, horizontal and
  vertical alignment of two points, parallel, perpendicular, equal,
  concentric, tangent (line–circle, circle–circle), symmetric about a
  line and about a point, point on line, point on circle, midpoint, fix;
  a point on or coincident with the sketch origin and on either axis;
  dimensions: distance, horizontal and vertical distance, point–line,
  parallel-lines, angle, radius, diameter, and the circle–circle
  clearance that offset writes. Behind `unreached`, off by default: block,
  point on a perpendicular bisector, symmetric across a datum, arc
  length, three-point angle, distance to the X and Y axes and to a
  datum, angle with a datum, point–circle clearance, and mirroring about a
  datum axis. The conic and spline kinds are behind `conics` and
  `SnellsLaw` behind `snells-law`. Their code and tests run in the
  all-features build until a scenario reaches them (`SEED.md` §8.2 rule
  7); a document holding one does not deserialise, and no fixture does.

## The viewport

`arrix-viewport` draws the scene into an offscreen target inside the egui
frame, through an `egui_wgpu` paint callback (`prepare` uploads, `paint`
draws). Ported, reviewed on the way in (`SEED.md` §8.1).

- **The renderer.** One `Renderer` per window holds the pipelines and the
  offscreen colour and depth, sized to the viewport's rect in physical
  pixels; what differs per document, the `Scene` and the camera, is passed
  to `Renderer::paint`. `prepare` draws into the offscreen target (egui's
  pass has no depth attachment), `paint` copies it into egui's pass one
  texel to a pixel. The pass draws a background gradient keyed to the world
  direction each pixel looks along, so the view's orientation reads even on
  the empty start; then each visible body's faces, one draw per body at its
  own dynamic offset in one model-matrix buffer; then every body's edges as
  lines. An edge lies on its faces and Vulkan's depth bias does not apply
  to lines, so its points are moved toward the eye along their own view
  ray by 5e-4 of their depth; a fixed offset in clip depth is decimetres a
  few metres out and lets hidden edges show past the corners that hide
  them. Faces are lit by a light fixed in the world, Z-up.

- **Scene per body.** A `SceneView` is a list of bodies, each keyed by its
  slot, with its mesh, transform, visibility and bounds. Updating one body
  re-uploads only it. The upload is behind a payload trait so the
  bookkeeping is tested without a GPU. Each body also keeps the box of its
  faces per feature their names start at (`NameRoot::feature`, whatever
  later features did to them), so `Scene::feature_bounds` says where a
  feature's own faces are on a body it was joined into. A frozen feature's
  faces keep their live names (a frozen body is named by its stored
  table), so this is where its badge goes.
- **Instances** (ADR-0030 §2). A body's mesh is uploaded once and drawn
  once per `Instance { path, key, pose }`: an assembly's context sets the
  scene's instances, each a component path, the body it draws and the
  component's world pose (a `Frame`), composed in f64 with the mesh's
  origin and cast to f32 last. With none set (a part's context) each
  visible body draws once at its own origin, with an empty path; both are
  `Scene::visible`'s `Drawn` instances, so the draw, pick, highlight,
  bounds and badge paths are one. Moving an instance re-uploads nothing.
- **Meshes** arrive in `EvalEvent`s as `RenderMesh`: f32 positions
  relative to the body's local origin (the kernel works in f64; the cast
  happens where the buffer layout is known), normals, face ranges and edge
  polylines, each range carrying its entity's `PersistentName`.
  Tessellation comes in two passes: a coarse mesh first so the body shows
  at once, then a refined one.
- **Picking by ID buffer.** On a frame with a click, a pick pass draws
  every face, edge and vertex as a `u32` id into an `R32Uint` target with
  its own depth: the kind in the top two bits, the instance's index among
  the visible instances in the next ten, the entity's index in its mesh's range
  table in the low twenty (`arrix_viewport::pick`; `0` is nothing, and a
  body or entity past those bits draws as nothing). The kind and index are
  vertex data, uploaded with the mesh; the body index is in the body's
  per-draw uniform, so hiding a body re-uploads nothing. Edges and vertices
  (points) are drawn after every face, moved toward the eye as the edges
  are, so what wins a pixel is what shows there. The 7 × 7 pixels about
  the cursor are copied out on the pass's own encoder and read back
  asynchronously (`map_async`, the device polled each frame, never
  waited on); the region ranks vertex over edge over face, then the
  nearest to the cursor, and resolves to a `Hit { path, name }`, the
  instance's component path and the `PersistentName` through the names the
  scene keeps per body, only if the body still holds the mesh the pass
  drew (a refined mesh may land in between). A click on a card
  never reaches the viewport (a layer above). The result is
  `UiIntent::Picked { name }`, `None` for empty space, and the selection is
  session state per tab. The path is the same on native and wasm, and
  needs nothing from the document, so it stays client-side in remote mode.
- **Hover and selection** restyle index ranges, over the finished scene
  and under no screen-space pass. A mesh keeps, beside its buffers, where
  each face (its range of the index buffer), edge (its range of the line
  list) and face outline (a range of one more line list, made once per
  mesh from the sides only one triangle of the face has) sits
  (`arrix_viewport` `HighlightRanges`). `Renderer::paint` takes the hovered
  and the selected `Hit`, `Scene::locate` finds each among the visible
  instances by its path and name, and the draw is the range again in a style: a face as a
  tint over its shading, with its outline; an edge as its line; a vertex as
  its point. Lines and points are a pixel thin, so each is drawn five
  times a pixel apart (a cross three pixels wide). Everything is moved
  toward the eye as the edges are, so it shows over what it lies on, and
  nothing is re-uploaded. Hover is amber and selection blue, selection
  drawn over hover; a name no visible body holds draws nothing and is never
  re-bound to another entity. Hover is a pick of its own: asked for when the
  pointer has moved over the viewport and none is on its way, answered as a
  click's is (`UiIntent::Hovered { name }`), and cleared when the pointer
  leaves; a click's pick wins over it. The status line names the hovered
  entity and what a click does. `core.selection.clear` (Escape, in `part`)
  clears the selection. None of it is a command on the document.
- **Camera**: orbit about a target, pan, zoom to cursor, perspective or
  orthographic, standard views, animated transitions that tests can skip.
  **Z-up, right-handed** (`SEED.md` §8.2). The camera is session state per
  tab, so it issues no command. Gestures are fixed for now (bindings in the
  keymap are a backlog line): middle drag (or right drag) orbits, shift and
  middle drag pans, the wheel zooms toward the cursor, left click picks. The
  view commands are `core.view.{home,top,front,right,iso,projection.toggle}`
  (`home`, `ctrl+7`, `ctrl+1`, `ctrl+3`, `ctrl+0`, `ctrl+5`); each flies
  the camera, and `ArrixApp::skip_animations` finishes a flight at once for
  a scenario. A pose that changed re-asks for the hover pick under a
  resting pointer. The ViewCube lives in `arrix-ui` (`viewcube`) and is
  painted with egui's painter, over the scene in the viewport's top-right
  corner, moved left of the property card while that shows. It is the
  chamfered cube's 26 facets (6 faces, 12 edges, 8 corners, one per
  `ViewOrientation`) in a fixed orthographic projection of its own, so it is
  one size whatever the camera does and takes only its orientation; faces
  carry their label in their own plane, the world axes stand on the
  (−X, −Y, −Z) corner, four step arrows turn the view 15°, and a button
  names and toggles the projection. A drag on the cube orbits at the
  viewport's own rate. It never writes the camera: it returns a
  `ViewCubeAction` (`Select`, `Orbit`, `Step`, `Home`, `ToggleProjection`)
  that the app applies to the camera it owns.
- **Finished sketches in part mode** (`part_sketch.rs`). A tab keeps each
  `core.sketch` feature's last solved sketch, made again only when a newer
  evaluation of it arrives. One that no feature takes a region or an item
  of (`Ref::Region`, `Ref::Sketch`) is drawn on its plane through
  `PlaneToScreen`, curves and points only, over the scene; one that is
  taken is hidden, and so is the sketch open in sketch mode, which draws
  its own. While the active form field asks for a `region`, `point` or
  `axis`, every sketch is drawn, and the pointer finds only that kind in
  them (`arrix_ui::pick_sketch_item`: the innermost region, the nearest
  point, the nearest straight line), the hovered region shaded and
  outlined as in sketch mode. An item under the pointer wins over the
  scene's pick, for hover and for click, and never otherwise: with no
  field asking the scene's pick is all there is. A click on one is a pick
  into the field (`Ref::Region` with the key the evaluator gave,
  `Ref::Sketch`), no selection. What the form's fields hold is drawn
  selected, and so does what an edit form's diagnostic names or offers
  (`debug_state().marked` lists the scene's). `debug_state()` lists the
  sketches drawn.
- **The create form's preview** (ADR-0019, `preview.rs`). While a create
  form's draft is complete enough to evaluate (every reference field shown
  holds one, and its texts parse as OK's would), the tab asks its session
  for a `Request::Preview` of the very command OK submits, once per change
  of the draft or of the generation; the draft's feature and part ids are
  minted once, so the preview's bodies and OK's are the same, cache hit
  included. A draft that stops being complete, Cancel and OK clear it. The
  scene keeps the preview's bodies in a list of their own: each new or
  changed body is drawn tinted green, whole, and a document body the
  preview changed or consumed is left out of the colour pass. The preview
  is never picked, located or framed: the pick pass still draws the
  document's bodies, replaced ones included, so a pick while it shows
  lands on what the document holds. Its events are matched to the last
  preview asked for, a superseded one's dropped. The form's header shows
  the draft's outcome in the preview (evaluating until it is heard, then
  its state and diagnostic, or the authority's refusal). Nothing of it is
  a command, and nothing waits on it: its answer arrives as events.
- **Overlays**: the sketch (points, curves, constraint glyphs,
  dimensions), plugin drawables, diagnostics highlights, frozen badges on
  bodies from frozen features.
- **The frozen badge** is a chip ("Spur gear · frozen", or "frozen,
  stale") painted over the viewport at the centre of a frozen feature's
  faces on each body they are on (`Scene::feature_bounds`, projected by the
  app), so a joined body carries it at the gear and not at its own centre.
  It is not drawn in sketch mode, where the pointer is the sketch's. A chip
  takes the clicks on it (a click on a badge picks nothing behind it), and
  hovering it puts the reason, in the form's words ("Feature type
  gears.spur is not available."), in the status line.
  `debug_state().badges` lists the badges drawn, in points. Only a frozen
  feature has one: failed, suppressed and rolled-back features show in the
  tree alone, and the preview's bodies have none.

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
  A failure's message names the kernel call by its count in the session,
  which previews make vary, so a golden reads `kernel call N` for it.
  `forms_accept` is the property card's: a plate made, edited, filleted
  and re-bound through the forms alone, its golden taken on the lost
  fillet's form (`debug_state().shell.form`, `marked`, `preview`).
  `frozen_badge` is the badge's: the slice opened without `gears` shows the
  chip at the gear, hovering it gives the reason, and the plate made
  thicker through its form makes it stale (`frozen_badge`,
  `frozen_badge_stale` goldens; `debug_state().badges`).
  `m3_accept` is M3's: a plate sketched with its origin inference shown,
  extruded through its preview, a face picked, a datum plane made from it,
  a spur gear from the Gears tab on that plane, the two joined, a fillet
  too big for its edge failing in the tree while the rest evaluates, and
  the radius made to fit; then the document reopened as a build without
  `gears` would: the gear frozen in the tree, its form read only with the
  reason, the badge on its faces, the final body the live run's; the plate
  made thicker through its form, the gear frozen-stale in the tree and the
  badge. Every gesture is one command or none, and the gesture log adds up
  to the commands each document applied (goldens `m3_accept_preview`,
  `m3_accept_gear`, `m3_accept_failed`, `m3_accept_frozen`).
- **The reference adapter is a CPU one**, Mesa's lavapipe. The harness
  requires it: a hardware GPU rasterises differently, so a scenario is
  never run on one. Without a CPU adapter a scenario skips, written
  straight to the process's stderr so the test runner's capture cannot
  hide it; with `ARRIX_REQUIRE_GPU=1` (CI) it fails instead.
- **Frames are deterministic on lavapipe**: two app instances, a
  re-render after more frames, separate processes and any lavapipe thread
  count (`LP_NUM_THREADS`) give identical pixels (measured on Mesa 25.2.8;
  `empty_shell_renders_identically` holds the in-process half). The coarse
  frame's tolerance exists for a different Mesa version, not for
  run-to-run noise.
- **No image is committed** (ADR-0001). Goldens are text; images are build
  output, re-rendered from any commit on demand.
- **The golden** is `{"debug_state": …, "coarse_frame": {"cell", "size",
  "rows"}}`, pretty-printed; `debug_state` is the app's JSON with its keys
  sorted. A failure reports the first `debug_state` difference as a JSON
  pointer with both values, and the coarse frame's differing regions.
- **The coarse frame** is the rendered frame reduced to a grid of 16×16
  pixel cells, each the cell's mean RGB, written as one string of six hex
  digits per cell per grid row. A cell the frame's edge cuts short averages
  the pixels it holds, so 1440×900 is 90×57 cells, about 31 KB of text that
  deltas well in git. A cell differs when any channel of its mean moved by
  more than the **tolerance, 8** (of 255); a scenario fails when more cells
  differ than its **budget, 0** by default. Both are overridable per
  scenario (`snapshot::check_with` and a `Tolerance`). Frames are identical
  run to run, so the tolerance only absorbs another Mesa version's edge
  rasterisation, a few levels per pixel averaged over 256 pixels; CI's
  first run checks that. The failure names the differing cells as pixel
  rectangles, one per 4-connected region, cell-aligned (`x 0..1440, y
  864..900 (270 cells, max delta 63)`). It catches layout, placement, colour
  and gross rendering changes; a change smaller than a cell (a 1-px
  outline) is below it, and text is `debug_state()`'s to catch.
- **Images on demand.** Every run writes the full frames to
  `target/snapshots/<name>.png` (`ARRIX_SNAPSHOT_DIR` redirects them).
  `scripts/snapshot-baseline [<rev>]` (default `HEAD`) renders the same
  scenarios at that revision in a temporary `git worktree`, built in
  `target/baseline/` (a shared target directory would hand one tree's
  test binary to the other), and moves its frames to `<name>.before.png`;
  it then runs the working tree's scenarios with `ARRIX_SNAPSHOT_DIFF=1`,
  which writes `<name>.diff.png` (every pixel whose RGB differs in
  magenta, over the frame dimmed to a quarter) and prints how many pixels
  differ. A scenario that fails its golden in either run still leaves its
  frames. The worktree is removed on exit, even on failure. CI uploads all
  three images as artifacts when a scenario fails.
- **`debug_state()`** is JSON of what the app drew and why, floats rounded
  to six places: the camera (eye, target, projection, matrices), the
  evaluation (generation, each feature's state and diagnostic), the bodies
  drawn (triangle, face, edge and vertex counts, bounds), selection and
  hover as persistent names, the open sketch (points in plane and screen
  coordinates, constraints, DoF, solver status), the ribbon and palette,
  and each plugin form and panel rendered. It is how the agent tells which
  number is wrong, not only that a pixel is. Built so far: the window
  size, the rect of each shell region (`layout`), the `ShellView` drawn
  (`shell`), the effective theme's name, the keymap in force and the config
  diagnostics; for the active document its `evaluation` (generation,
  finished, settled), `bodies` (counts and bounds), `camera` (pose,
  projection and the 16 numbers of its view-projection), `selection`,
  `hover` and whether a `picking` is in flight, each left out while empty;
  the sketch joins as its part of the app is built. While a create form
  previews, `preview` says whether its outcome is `heard` and its bodies
  drawn refined (`settled`), its `bodies` as `bodies` lists the
  document's, and the document bodies it `replaced`.
- **A mesh without a document**: `ArrixApp::show_mesh(key, mesh)` puts a
  `RenderMesh` on screen as a `Meshed` event will; the scenarios' fixture
  box (`tests/visual/fixture.rs`) pins the renderer apart from the kernel.
- **Deterministic frames.** No wall clock reaches a drawn pixel in a test:
  animations are skipped, the frame-time readout is off, and evaluation is
  waited for, not timed.
- **The scratch render** (`cargo test -p arrix-app --test visual_scratch`)
  draws a state to `target/visual-scratch/scratch.{png,json}` and compares
  against nothing: the agent's way to look before it pins a scenario.
- **The product render** (`arrix-app render <doc> -o <png> [--json
  <file>] [--view home|top|front|right|iso] [--context <id|name>] [--size WxH]
  [--plugin <dir>]… [--without-plugin <id>]…`, ADR-0025) is the same drawing outside the test
  harness: the real app on its own `egui::Context` (`ArrixApp::headless`),
  stepped as the harness steps it, painted through `egui_wgpu` into a
  texture and read back, after the document has settled. `--view` runs the
  app's view command once it has; `--context` looks at a part or an
  assembly by id or name instead of where the document opens (its first
  root assembly, else its first part); `--size` is the window in pixels
  (1440×900 by default); `--plugin` starts a Tier 2 plugin for the
  document and `--without-plugin` leaves one out, so a frozen feature draws
  with its badge, as `arrix eval`'s flags do. The JSON is `{format, adapter,
  debug_state}`, `format` being `render::FORMAT` (1). A document that does
  not open, a plugin that does not start, or no adapter exits 1 with the
  reason on stderr. It prefers a CPU adapter and draws on any other; its
  pixels are promised only on lavapipe. `render_parity` holds it to the
  harness: the slice drawn by both on one device gives identical pixels
  and the same `debug_state()`; `render_cli` tests each flag on the
  binary.
- **The render JSON is a public interface** (ADR-0026). `arrix-app render
  --schema` prints its schema, `render::schema()`, held by a golden that
  `render_cli` validates every render against. The schema marks the
  promised fields of `debug_state()` `x-arrix-promised`: `bodies`,
  `camera`, `badges`, `evaluation` and `diagnostics`; and, marked
  `x-arrix-promised-when` (present only under the condition it names),
  `context` and `instances` of an assembly (C2, additive, `Harness: render
  1, additive`). Everything else in
  it is informational and may change without a version bump, so a scenario
  golden may pin it and an agent should not. Changes inside `format` 1 are
  additive only.
- **Refreshing a golden** (`UPDATE_SNAPSHOTS=1`) is done only for an
  intentional UI change, after reading the before, after and diff images
  rendered on demand and the golden's text diff, and the commit says why
  (`.agents/rules/git.md`).
- **Pitfalls** the harness encodes, so no one relearns them: a frame step
  without a render does no GPU work; a click is one event per rendered
  frame; evaluation is asynchronous, so settle before asserting; a camera
  flight runs on a wall clock, so a test finishes it with
  `ArrixApp::skip_animations`; an answered pick is drawn by the frame after
  it, so step once before reading pixels or the hint; the harness's `drop_at`
  removes the pointer, which clears hover, so hover again before asserting a
  hint; a document with repaints pending never lets `run()` finish, so use
  `run_steps`.
- The `visual-debug` skill (`.agents/skills/visual-debug/SKILL.md`) is
  the agent's how-to: scratch or scenario, reading the golden, the knobs,
  the baseline before any refresh, and these pitfalls.

## Open questions

None yet.
