# Roadmap

A cycle retires the scariest remaining unknown first and ends in numbers
(`SEED.md` §7). Its "out" list is as binding as its "in" list: work outside
the open cycle's in-list needs an ADR naming the product scenario it unlocks
before its plan starts. One section per cycle; a finished cycle compresses
to its status line (`/close-cycle`), and the next is detailed below it.

Spine: **C1** M0 → M1 → M2 → M3 → M4 (the vertical slice with a plugin
feature in it; not started), then **C2** (the mechanical core), **C3** (the
boundary crossed), **C4** (the web). Beyond C4, the named cycles are
unordered.

---

## C1 — the vertical slice, with a plugin feature in it

*Goal: a part is sketched, extruded, cut, filleted, patterned and holed,
with a spur gear from a Tier 0 plugin in the same history, and everything
downstream of the gear follows an upstream edit. Saved and reopened without
the plugin, every body is there, the gear is frozen and says why, and the
features after it still evaluate. The risk retired: **a plugin feature is a
first-class history entry**. It has to be proven before anything is built
on a document model that might not allow it.*

**Status: not started. Seeded 2026-09-26.**

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
- `arrix-kernel`: the `Kernel` service for primitives, extrude and the
  booleans; naming from provenance for them; call records.
- `core.sketch`, `core.datum-plane`, `core.extrude` (new body, join, cut).
- `arrix-plugin-api` 0.1: the WIT world's `types`, `kernel` (the subset
  above) and `feature` interfaces, and the Rust traits a test holds equal
  to what `wit-bindgen` generates from the world. `arrix-plugin-host` Tier 0.
- `plugins/gears`: `gears.spur` (teeth, module, width, pressure angle, on a
  plane), its involute flanks approximated by arcs within a stated
  tolerance, which is the plugin's choice of profile and not a kernel
  workaround.
- Frozen results: stored on every live evaluation, used when the plugin is
  absent, stale after an upstream edit, re-frozen when it returns
  (`docs/DATA-MODEL.md` §Frozen results). Needs body bytes (A2).
- `.arrx` v1: the directory form and the zip, deterministic, with the
  migration mechanism and its first (identity) fixture.
- ⚠ OPEN closed here: undo granularity for plugin commands, with its ADR.

**Accept:** a scenario document built by commands alone: a sketched plate
extruded, a spur gear on a datum plane above it joined to it, a pocket cut
through both. Editing the plate's width re-evaluates everything after it
and every reference resolves. Save, save again: identical bytes. `arrix
eval --without-plugin gears`: the gear frozen with reason *missing*, the
pocket still cut, volumes identical to the live run. Edit the width
without the plugin: the gear frozen-stale, the pocket re-evaluated against
its stored result. Reopen with the plugin: re-frozen, not stale. The gear
evaluated twice gives identical bytes. Undo of every command in reverse
returns the document, byte for byte, to each earlier state.

### M2 — the part features and naming

- `core.revolve`, `core.fillet`, `core.chamfer`, `core.hole` (simple,
  counterbore, countersink; blind or through), `core.pattern-linear`,
  `core.pattern-circular`, `core.boolean`, and `core.mirror` once A11 is
  released (`docs/DATA-MODEL.md` §The core feature types).
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
- `arrix eval` complete: directories, `--sweep`, `--summary` histogram,
  `--save-fixtures`, `--without-plugin`, `--jobs`, `--budget` once A3
  lands (`docs/CONCURRENCY-WASM.md` §Batch evaluation).
- The C1 benchmark: the budgets of `docs/CONCURRENCY-WASM.md` §Budgets
  measured on the acceptance part and written there.
- Cancellation through A3's token, if released; otherwise between kernel
  calls, with the gap named.

**Accept:** the cycle's.

**Out:** assemblies, components, joints; materials and mass properties in
the UI; measure and section view; STEP, 3MF and DXF/SVG import (C2); Tier 1
and Tier 2 hosts, the Python SDK and the packaged test kit (C3); the
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
| A2 body bytes | M1 frozen results; M2 fixture operands that are bodies | M1's frozen steps wait; the rest of M1 proceeds. It blocks the cycle's accept |
| A11 `ops::mirror` | M2 `core.mirror` | `core.mirror` moves to the backlog; C1 closes without it |
| A3 cancellation | M4 cancellation latency, `--budget` | cancellation between kernel calls only; the gap named in the status line |
| A1 consumer roles | a plugin building topology directly | the gear extrudes and does not need it; the builder service waits |
| A5 `region2` | sketch regions shared with the kernel | the ported arrangement is used |

### C1 risk register

| Risk | Retired or watched by |
|---|---|
| A plugin feature cannot be first-class: frozen results, naming or memoisation need a special path | M1's accept, before any other feature is built |
| Body bytes (A2) arrive late | M1 ordered so the frozen steps come last; the ask is on record |
| Names do not hold through booleans and patterns | M2's naming scenarios, each with its opposite case |
| The sketcher port drags in scope | only UI-reachable constraint kinds enabled; the size lint's allowlist watched |
| The WIT world and the Tier 0 traits drift apart before C3 builds Tier 1 | a test in M1 holding the traits equal to the generated bindings |
| Replicas and serialised commands cost too much | the command-apply budget, measured in M4 |
| Headless GPU snapshots flake | scenarios serial, one adapter (lavapipe) as the reference, a missing adapter skips loudly, the coarse frame's per-cell tolerance absorbing driver noise |

---

## C2 — the mechanical core

Assemblies (components, instances, joints, FK), materials, mass
properties, measure, section view, STEP import as a feature with assembly
structure (A4), 3MF, DXF/SVG sketch exchange. Accept: a real multi-part
assembly round-trips through STEP against another CAD's reading of it.

## C3 — the boundary crossed

Tier 2 (Python) and Tier 1 (wasm component) hosts; robotics moved out as
plugin #1; a second first-party plugin in Python through Tier 2 only; the
plugin test kit; `arrix-plugin-api` reviewed for 0.x stability. Accept: the
north-star demo (`SEED.md` §7).

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
- Remote mode: a self-hosted server with thin native and browser clients
  (`SEED.md` §6.1): authentication, presence, transport. Branching and
  merging documents is a separate question.
