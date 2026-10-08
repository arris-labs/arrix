# Roadmap

A cycle retires the scariest remaining unknown first and ends in numbers
(`SEED.md` §7). Its "out" list is as binding as its "in" list: work outside
the open cycle's in-list needs an ADR naming the product scenario it unlocks
before its plan starts. One section per cycle; a finished cycle compresses
to its status line (`/close-cycle`), and the next is detailed below it.

Spine: **C1** M0 → M1 → M2 → M3 → M4 (the vertical slice with a plugin
feature in it; M0, M1 and M2 landed), then **the boundary opened** (the
agent's way in; ADR-0016, numbered by C1's `/close-cycle`), **C2** (the
mechanical core), **C3** (the boundary crossed), **C4** (the web). Beyond
C4, the named cycles are unordered.

---

## C1 — the vertical slice, with a plugin feature in it

*Goal: a part is sketched, extruded, cut, filleted, patterned and holed,
with a spur gear from a Tier 0 plugin in the same history, and everything
downstream of the gear follows an upstream edit. Saved and reopened without
the plugin, every body is there, the gear is frozen and says why, and the
features after it still evaluate. The risk retired: **a plugin feature is a
first-class history entry**. It has to be proven before anything is built
on a document model that might not allow it.*

**Status: M0 landed 2026-09-26: the gate, the harness and `arrix eval` on an
empty document. M1 plan 1 of 3 landed 2026-09-26: the document, commands
and per-author undo, the evaluator and `LocalSession`, plugin API 0.1,
Tier 0 and `gears.spur` referenced downstream by persistent name
(ADR-0004). M1 plan 2 of 3 landed 2026-09-27: `arrix-sketch` ported
headless less inference, `core.sketch` solved by the evaluator from
stored positions (ADR-0005), regions as keyed profiles referenced by
`Ref::Region`, plugin API 0.2 (ADR-0006). M1 plan 3 of 3 landed 2026-10-02: bodies as inputs and
modified slots (ADR-0007, ADR-0008), `core.extrude` in its three modes and
`core.boolean` in its minimal form, plugin API 0.3, names through `fuse`
and `cut`, frozen results written on save by the session (ADR-0009), the
frozen states, schema 2 with its migration chain, the deterministic `.arrx`
zip, `arris` 0.4 (A2), and the accept scenario green. M2 plan 1 of 2
(`c1-m2-naming`) landed 2026-10-06: the single-piece rule, the instance step
and the revolve root (ADR-0010), a sweep index in the sweep root (ADR-0011),
`revolve`, `fillet`, `chamfer`, `copy` and `mirror` named at the kernel, the
fixture export and `eval --save-fixtures`, `arris` 0.5, and the kernel naming
scenarios green. M2 plan 2 of 2 (`c1-m2-features`) landed 2026-10-07: list
inputs and the `edge`, `point`, `axis` and `tool` kinds (ADR-0012), a
pattern or mirror of features copying their tools (ADR-0013), a boolean
number in the step (ADR-0014), `core.revolve`, `core.fillet`,
`core.chamfer`, `core.hole`, `core.pattern-linear`, `core.pattern-circular`,
`core.mirror` and `core.boolean` in full, projection into a sketch, schema
3, plugin API 0.4, and the accept green
(`crates/arrix-cli/tests/naming_scenarios.rs`, `save_fixtures.rs`). M3
plan 1 of N (`configurable-shell`) landed 2026-10-07: the command
registry and keymap, the palette, themes, feature parameter defaults, `arrix
describe` and `arrix config check|schema|catalog`, reload, ADR-0015 and
ADR-0016, and the `configured_shell` accept. M3 plan 2 (`viewport-port`)
landed 2026-10-07: the minimal shell (ADR-0017), document tabs, tessellation
at the kernel and `Meshed` events, the wgpu viewport with ID-buffer picking,
hover and selection, the camera and the ViewCube, and the `viewport_accept`
scenario. M3 plan 3 (`sketch-mode`) landed 2026-10-08: the drag inside its
4 ms budget (gated), inference ported, undo, redo and new document, entering
and leaving sketch mode, the `SketchView` overlay, the drawing, constraint,
dimension and modify tools, regions shaded as extrude takes them, the
unreached constraint kinds gated, and the `sketch_accept` scenario. M3
plan 4 (`forms`) landed 2026-10-08: forms rendered from the feature type's
spec (ADR-0018), choices and `shown_when` in plugin API 0.5, previews as
speculative evaluations (ADR-0019), the create and edit forms, references
picked in the scene and the tree, finished sketches in part mode, lost-reference
and failure diagnostics on the form, and the `forms_accept` scenario. M3
continues with its remaining accept (frozen badge, gesture-count test).
Seeded 2026-09-26.**

Milestones are ordered by risk after the harness: the document and the
plugin feature are proven headless (M1) before the part features (M2), the
interactive app (M3) and exchange and batch (M4) are built on them. Each
milestone is one or more plans; each ends with its own executable check.

### M0 — the gate and the harness

Everything later is checkable because this comes first (`SEED.md` §10.3).

- The workspace with every crate of `docs/ARCHITECTURE.md` §Crates as a
  compiling stub, and `arrix-core`'s units, ids and `Diagnostic`.
- The gate: `.githooks/pre-commit` and `pre-push`, CI mirroring them, the
  size lint, the wasm lint and the new layer lint (`docs/ARCHITECTURE.md`
  §Gates). `AGENTS.md` §Commands written.
- The headless visual harness ported: `egui_kittest` on wgpu, text
  goldens (`debug_state()` and the coarse frame, ADR-0001), images on
  demand with `scripts/snapshot-baseline`, the scratch render, the
  `visual-debug` skill, on an empty shell (ribbon, tree, viewport, property panel,
  status bar).
- `arrix eval` reading an (empty) document directory and printing its
  JSON line.

**Accept:** the gate green on a clean clone; a deliberate violation of each
lint (an oversize function, `Instant::now` in a core crate, `egui` in
`arrix-doc`, a second `arrix-*` dependency in a plugin) fails it; one
snapshot scenario of the empty shell; `arrix eval` golden on an empty
document.

### M1 — the document, and a plugin feature in its history (the risk)

Headless throughout; no viewport yet.

- `arrix-sketch` ported and reviewed, headless: model, solver, diagnostics,
  regions, trim/extend/offset/mirror; constraint kinds limited to the ones
  M3's UI will reach, the rest behind gates (`SEED.md` §8.2 rule 7).
- `arrix-doc`: ids, parameters and expressions, parts and feature records,
  the DAG, commands with generations and per-author undo, `LocalSession`
  with replicas, the evaluator on its worker thread, the cache.
- `arrix-kernel`: the `Kernel` service for extrude, `fuse` and `cut` and
  body bytes; naming from provenance for them; call records. Primitives
  join with the first feature that calls one (M2).
- `core.sketch`, `core.datum-plane`, `core.extrude` (new body, join, cut)
  and `core.boolean` in its minimal form (one target, one tool, `fuse` or
  `cut`; M2 extends it to `common` and several tools), which is how the
  accept joins a plugin's body to a sketched one.
- `arrix-plugin-api` 0.1, and 0.2 with region references (ADR-0006):
  the WIT world's `types`, `kernel` (the subset above) and `feature`
  interfaces, and the Rust traits a test holds equal to what
  `wit-bindgen` generates from the world. `arrix-plugin-host` Tier 0.
- `plugins/gears`: `gears.spur` (teeth, module, width, pressure angle, on a
  plane), its involute flanks approximated by arcs within a stated
  tolerance, which is the plugin's choice of profile and not a kernel
  workaround.
- Frozen results: stored on every live evaluation, used when the plugin is
  absent, stale after an upstream edit, re-frozen when it returns
  (`docs/DATA-MODEL.md` §Frozen results). Needs body bytes (A2).
- `.arrx` v1: the directory form and the zip, deterministic, with the
  migration mechanism and its first (identity) fixture.
- Closed here: undo granularity for plugin commands (ADR-0004).

**Accept:** a scenario document built by commands alone: a sketched plate
extruded, a spur gear on a datum plane above it joined to it, a pocket cut
through both. Editing the plate's width re-evaluates everything after it
and every reference resolves. Save, save again: identical bytes. `arrix
eval --without-plugin gears`: the gear frozen with reason *missing*, the
pocket still cut, volumes identical to the live run. Edit the plate's
thickness (which moves the gear's plane; the width does not) without the
plugin: the gear frozen-stale, the pocket re-evaluated against
its stored result. Reopen with the plugin: re-frozen, not stale. The gear
evaluated twice gives identical bytes. Undo of every command in reverse
returns the document, byte for byte, to each earlier state.

### M2 — the part features and naming

- `core.revolve`, `core.fillet`, `core.chamfer`, `core.hole` (simple,
  counterbore, countersink; blind or through), `core.pattern-linear`,
  `core.pattern-circular`, `core.boolean`, and `core.mirror` (A11 is
  released in `arris` 0.4.0) (`docs/DATA-MODEL.md` §The core feature types).
- Persistent naming through every one of them, sketch entity ids carried
  into sweep roots; lost references as diagnostics with ranked candidates,
  never a rebind.
- The kernel's sensor: failure categories, the failing call record kept
  beside the diagnostic, and its export as an Arris fixture recipe
  (`docs/ARCHITECTURE.md` §The kernel choke point).
- Projection of edges and vertices into a sketch, re-projected on
  evaluation.

**Accept:** naming scenarios, each an edit followed by a check that a
downstream reference still resolves to the same entity: a width edit under
a fillet, a hole moved across the plate, a pattern count raised and
lowered, a sketch curve redrawn between two kept ones, a feature inserted
before a filleted edge. Each of the opposite cases (the referenced edge
removed) ends in a lost-reference diagnostic with candidates, and the
features that do not depend on it still evaluate. A fillet whose radius is
too large fails with `kernel.degenerate.blend-too-large`, and the recipe
`--save-fixtures` writes reproduces that error in Arris's fixture runner.

### M3 — the interactive app

- `arrix-viewport` ported and reviewed: scene per body, coarse then refined
  meshes, ID-buffer picking resolving to persistent names, hover and
  selection, the camera, the ViewCube.
- The shell: the command registry, the ribbon with the gears plugin's tab,
  the command palette, the feature tree with its states, the property
  panel rendered from forms (the gear's included), diagnostics.
- The shell configured by user files (ADR-0015): keymap, themes, hidden
  ribbon tabs and feature parameter defaults in TOML, `arrix config
  check`, `arrix describe`, reload without restarting.
- Sketch mode: drawing, inference that commits what it shows, dimensions
  as expressions, trim/extend/offset/mirror, regions shaded as extrude
  will take them.
- One gesture = one command, checked: every gesture in the acceptance flow
  issues exactly one command, or none when nothing changed.

**Accept:** snapshot scenarios (text goldens, ADR-0001) of the
acceptance flow driven through the UI: a sketch with its inferences, the
extrude preview and result, a picked face, the gear's form and tab, the
tree after a failed fillet, the frozen badge after reopening without the
plugin. A gesture-count test over the flow.

### M4 — exchange and batch evaluation

- Export: STEP of any set of bodies, STL and OBJ of their render meshes
  (`arris_io`).
- `arrix eval` complete (`--save-fixtures` landed in M2): directories,
  `--sweep`, `--summary` histogram, `--without-plugin`, `--jobs`,
  `--budget` once A3 lands (`docs/CONCURRENCY-WASM.md` §Batch evaluation).
- The C1 benchmark: the budgets of `docs/CONCURRENCY-WASM.md` §Budgets
  measured on the acceptance part and written there.
- Cancellation through A3's token, if released; otherwise between kernel
  calls, with the gap named.

**Accept:** the cycle's.

**Out:** assemblies, components, joints; materials and mass properties in
the UI; measure and section view; STEP, 3MF and DXF/SVG import (C2); the
Tier 2 host, `arrix serve`, the Python SDK and the packaged test kit (the
boundary opened); the Tier 1 host (C3); the
browser running the app (C4: C1 only keeps it compiling); sweep, loft,
shell and draft (Arris's sweep cycle); the robotics plugin (by C3); network
code of any kind, upstream failure reporting included (`docs/BACKLOG.md`);
parallel evaluation inside one document; a Tier 0 plugin drawing its own
egui.

**Accept:** one scenario, run by `cargo test` and by `arrix eval` on its
saved document:

1. A bracket built through the commands the UI issues: a sketched plate
   extruded; a pocket cut; its edges filleted; a counterbored hole linearly
   patterned; a spur gear from `plugins/gears` on a datum plane, joined;
   an edge of the gear's face filleted downstream, referenced by persistent
   name. Volume, area and counts match the scenario's golden JSON.
2. The plate's width edited: every feature re-evaluates, no reference is
   lost, the names of faces the edit did not touch are unchanged.
3. Saved twice to identical bytes; loaded and evaluated to identical JSON.
   Undo and redo of every command reproduce each earlier file byte for
   byte.
4. Opened without the plugin: every body present with the live run's
   volume; the gear frozen with its reason; the downstream fillet
   evaluated. The width edited again: the gear frozen-stale, the rest
   re-evaluated. Opened with the plugin: re-frozen.
5. A fillet made too large fails with its category, the rest of the part
   evaluates, and its saved fixture recipe reproduces the failure in
   Arris.
6. STEP export read back by `arris_io::step::read` to the same counts and
   to the volume within the bodies' tolerance; STL and OBJ closed, their
   volume within the mesh's tolerance.
7. `arrix eval --sweep` of the width over 50 values: one JSON line each,
   and the failure histogram printed.
8. The snapshot scenarios of M3 green; the budgets measured and printed;
   the whole gate green, the wasm build included.

### C1 Arris dependencies

The asks are in Arris's repository (`docs/ideas/arrix-consumer-asks.md`
there). A step that needs one waits for a released `arris` carrying it,
and names it; nothing is worked around.

| Ask | Needed by | If it has not landed |
|---|---|---|
| A2 body bytes | M1 frozen results (M2's fixture recipes replay the calls instead: Arris's recipe has no body-bytes operand) | landed: `arris` 0.4.0, taken in M1 |
| A11 `ops::mirror` | M2 `core.mirror` | landed: `arris` 0.4.0, taken in M2 (`core.mirror` in `plans/c1-m2-features`) |
| A3 cancellation | M4 cancellation latency, `--budget` | cancellation between kernel calls only; the gap named in the status line |
| A1 consumer roles | a plugin building topology directly | the gear extrudes and does not need it; the builder service waits |
| A5 `region2` | sketch regions shared with the kernel | the ported arrangement is used |
| (to file) a split's piece order by a consumer-given key, not input ids | a reference to one piece of a split face surviving a redraw of the curves that bound it (`plans/c1-m2-naming`, step 2's finding) | a split piece's name follows Arris's id order, as `docs/DATA-MODEL.md` §Persistent naming states |

### C1 risk register

| Risk | Retired or watched by |
|---|---|
| A plugin feature cannot be first-class: frozen results, naming or memoisation need a special path | retired by M1's accept (`crates/arrix-cli/tests/slice.rs`), before any other feature was built |
| Body bytes (A2) arrive late | retired: A2 landed in `arris` 0.4.0 and M1's frozen results use it |
| Names do not hold through booleans and patterns | retired: the kernel half by `crates/arrix-kernel/tests/naming_scenarios.rs`, the document half by `crates/arrix-cli/tests/naming_scenarios.rs`, each scenario with its opposite case |
| The sketcher port drags in scope | only UI-reachable constraint kinds enabled; the size lint's allowlist watched |
| The WIT world and the Tier 0 traits drift apart before C3 builds Tier 1 | a test in M1 holding the traits equal to the generated bindings |
| Replicas and serialised commands cost too much | the command-apply budget, measured in M4 |
| Headless GPU snapshots flake | scenarios serial, one adapter (lavapipe) as the reference, a missing adapter skips loudly, the coarse frame's per-cell tolerance absorbing driver noise |

---

## The boundary opened

The Tier 2 host, `arrix serve --stdio`, the Python SDK and the test kit;
the scenario is in ADR-0016 and C1's `/close-cycle` details the section.

## C2 — the mechanical core

Assemblies (components, instances, joints, FK), materials, mass
properties, measure, section view, STEP import as a feature with assembly
structure (A4), 3MF, DXF/SVG sketch exchange. Accept: a real multi-part
assembly round-trips through STEP against another CAD's reading of it.

## C3 — the boundary crossed

The Tier 1 (wasm component) host; robotics moved out as plugin #1; a
second first-party plugin in Python through Tier 2 only; `arrix-plugin-api`
reviewed for 0.x stability. Accept: the north-star demo (`SEED.md` §7).

## C4 — the web

The browser build with a worker executor; Tier 0 plugins only; a document
using Tier 1 or Tier 2 plugins opens with those features frozen
(`SEED.md` §7).

---

## Named cycles, unordered

None is scheduled. Each earns a section when `/close-cycle` opens it.

- Sweep, loft, shell and draft features (on Arris's sweep cycle).
- Interference checks (on Arris's query cycle).
- A drawings plugin.
- Sheet metal.
- A general assembly mate solver.
- Configurations.
- A plugin index.
- Distributions: a named, shareable composition of plugins, config and
  ribbon layout; after the runtime tier (ADR-0016).
- Remote mode: a self-hosted server with thin native and browser clients
  (`SEED.md` §6.1): authentication, presence, transport. Branching and
  merging documents is a separate question.
