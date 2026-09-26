# Project Seed Document: ArriX

## 1. Project Overview
**Name:** ArriX
**Tagline:** A free, pure-Rust parametric CAD with a small, solid core and everything else as plugins.
**Core Concept:** ArriX is a general-purpose parametric B-Rep modeller built on the [Arris](https://crates.io/crates/arris) kernel: sketches, features, bodies, assemblies and exchange formats, done well and nothing more. Everything a domain adds (robotics, drawings, sheet metal, FEA, CAM, BIM, standard-part libraries, rendering) is a plugin. First-party plugins get no private hooks and use the same API as everyone else. Plugins are written in Rust or Python. A plugin can add history features that behave like built-in ones. The same Rust codebase runs natively and in a browser.

Development is fully agentic, as in Arris. The human sets direction and judges results; the agent designs, implements, tests and documents. So everything ArriX does must be checkable without a human looking at a screen: headless rendering, a JSON dump of what the app drew, a CLI that evaluates a document, and a plugin test kit.

ArriX starts as a clean repository. Existing code that holds up (a sketch solver, a viewport, a headless visual harness) is ported and reviewed on the way in, and lessons from earlier work are principles in this seed (§8), not history to read.

## 2. The Naming Rationale
An **arris** is the sharp edge where two surfaces meet. The kernel computes it; **ArriX** is what you build with it. The X is the extension point: the application exists to be extended.

Registry check on 2026-09-26: `arrix` and `arrix-core` are free on crates.io, and `arrix` is free on PyPI. The project lives beside the kernel on GitHub, at `Divelix/arrix` next to `Divelix/arris`, until a GitHub organisation for the family is chosen; both repositories move into it then, and first-party plugins become repositories there once they leave the tree (§6.2; ADR-0002). Reserving names is the human's act.

## 3. The Problem Statement
There is no free CAD that is both **solid at its core** and **cleanly extensible**:

* **FreeCAD** is extensible but has no core/plugin boundary. Workbenches reach into each other and into the C++ core through Python bindings of internals. The API is whatever the code happens to be, so a core refactor breaks the ecosystem and the ecosystem blocks core refactors. Its kernel (Open CASCADE) gives no provenance, so topological naming was a 15-year problem, patched heuristically in 1.0.
* **SolveSpace and Dune3D** are small and coherent, with no plugin story. What they don't do, you can't add.
* **Code-CAD** (CadQuery, build123d, OpenSCAD) is scriptable but not interactive. A script is not a document another person edits with a mouse.
* **Onshape's FeatureScript** proves the right idea: a user-defined feature is a first-class history entry, regenerated like a built-in one. But it is proprietary and cloud-only, in a language nobody else runs.
* **Fusion, SolidWorks and Onshape add-ins** are proprietary and can't be inspected, forked or run offline.

Any CAD built on Open CASCADE or truck inherits a kernel it can't change, and its architecture ends up shaped around that kernel's gaps. ArriX is built on a kernel developed by the same people, with the same method, so a kernel gap is a kernel issue rather than an application workaround.

So the problem is not "write a CAD". It is: **define the smallest core a general CAD needs, make it robust, and put a stable, versioned, sandboxable boundary around it that Rust and Python plugins can extend, down to history features.**

## 4. Competitive Landscape (as of September 2026)

| Tool | What it is | Why it does not close the gap |
|---|---|---|
| SolidWorks, Fusion, Onshape, NX | Commercial parametric CAD with add-in APIs | Proprietary; the bar for core features, not a competitor for the niche |
| [FreeCAD](https://www.freecad.org/) 1.x | The free general-purpose CAD: OCCT, Python, workbenches | Extensible without a boundary (above); OCCT's limits; TNP patched on top |
| [SolveSpace](https://solvespace.com/), Dune3D | Small, coherent parametric modellers | No plugin system; limited features |
| CadQuery, build123d, OpenSCAD | Code-first modelling | Not an interactive document; no GUI history |
| Onshape FeatureScript | User-defined features as history entries | Proprietary, cloud-only, own language. **The precedent for §6's plugin features** |
| Zoo (KittyCAD) | Code + GUI CAD on its own kernel | Kernel proprietary and cloud-hosted |
| Plasticity | Direct modeller on Parasolid | Proprietary kernel; not parametric |

**Why ArriX anyway, in priority order:**
1. **Plugins that are first-class.** One API, versioned apart from the app. A plugin can add a history feature, document data, an importer or exporter, a command, a panel or an analysis. A document opens without the plugins that made it (§6.4).
2. **A kernel with provenance.** Arris reports what every output came from, so persistent names are computed rather than guessed. A plugin's features get the same naming for free.
3. **Pure Rust, native and web, one codebase.** No C++ toolchain, no cloud dependency.
4. **Built to be driven by agents.** Headless everything; a CLI and a Python API that are the plugin API; a document format a diff can read.
5. **Every failure feeds the kernel.** A kernel failure in ArriX is a replayable record that can become an Arris fixture (§6.6). The kernel's corner-case corpus grows from real use, generated designs and replayed datasets, rather than only from what its developers think to test. That is how a young kernel closes the gap to 30 years of customer bug reports.
6. **MIT OR Apache-2.0**, like Arris: plugins may carry any licence, commercial ones included.

**Explicit non-goals of the core:** anything that is a domain (§6.2). This does not mean they are out of scope for the project: first-party plugins exist and are maintained here. Also not in the core: a physics or FEA solver, rendering beyond a modelling viewport, a hosted cloud service, and direct (non-history) modelling as a headline mode.

## 5. The Solution & Developer Experience

For a **user**, ArriX is a parametric modeller: sketch on a plane or face, extrude and cut, fillet, pattern, assemble, import a vendor STEP and export. A ribbon and a command palette hold every command, the plugins' included. Installing a plugin adds a ribbon tab, never a mode you switch into.

For a **plugin author**, a history feature is one trait (Rust) or one class (Python):

```rust
// Tier 0/1 — Rust. Only `arrix_plugin_api` types; no Arris, no egui.
pub struct SpurGear;
impl Feature for SpurGear {
    const ID: &'static str = "gears.spur";          // namespaced, stable
    fn params(&self) -> Form { form![ int("teeth", 20), length("module", mm(1.0)), length("width", mm(8.0)) ] }
    fn inputs(&self) -> Inputs { inputs![ plane("on") ] }
    fn evaluate(&self, cx: &mut EvalCx, p: &Params, i: &Resolved) -> Result<Output, Diagnostic> {
        let profile = cx.profile().involute_gear(p.int("teeth"), p.length("module"))?;
        let body = cx.kernel().extrude(&profile, i.plane("on"), p.length("width"))?;
        Ok(Output::body(body))                      // provenance and naming come from the kernel
    }
}
```

```python
# Tier 2 — Python, out of process: numpy, torch and CUDA are available.
import arrix

@arrix.analysis(id="fea.linear_static", title="Linear static")
def solve(cx: arrix.AnalysisCx, doc: arrix.Snapshot) -> arrix.FieldOverlay:
    mesh = cx.volume_mesh(doc.body("bracket"), size=arrix.mm(2))
    stress = my_gpu_solver(mesh, doc.plugin_data("fea"))    # the author's own code
    return arrix.FieldOverlay(mesh, stress, unit="MPa")
```

A **script** is a Tier 2 plugin that runs once: `arrix run script.py doc.arrx`. An **agent** drives ArriX through the same Python API or the CLI. It never has to click.

## 6. Architecture Commitments

### 6.1 The one data flow
UI input → `Command` → the document mutates (undo entry, generation bump) → the evaluator replays what changed **off the UI thread** → `EvalEvent`s → scene and panels refresh. A plugin mutates the document only through commands and gets results only through events, like the UI does. The kernel never runs on the UI thread; the sketch solver is the one sanctioned exception.

**The boundary is a protocol, even locally.** A future mode, not built soon, is a self-hosted **server with thin clients**: the document, evaluator, kernel and plugins run on one powerful machine, and several people edit the same document from weak laptops or a browser. Each client renders, picks and solves sketches locally; everything else is on the server. Onshape works the same way. The architecture must not preclude it, so four rules hold from day 0:

1. **Commands, `EvalEvent`s and view types are plain, serialisable, versioned data.** No `Rc`, closures, kernel handles or pointers into the document cross the boundary. Meshes cross with their persistent names, so picking stays client-side.
2. **Undo is per author, by inverse commands.** Undo never restores a snapshot, which would undo other people's work.
3. **A command names the document generation it was made against.** The document, the single authority, applies it, rebases it or rejects it as stale. There is one writer, so no CRDTs are needed.
4. **Session state belongs to the client**: selection, the active sketch, camera and edit mode are not in the document. The evaluator publishes events to subscribers and never assumes a single UI.

**Local mode is remote mode with an in-process transport.** The boundary is exercised on every run, so it cannot rot. Tier 0 plugins going through the same traits as the WIT bindings is the same trick. No network code exists until the mode is built. Declarative plugin UI (§6.3) is what makes plugins work remotely, and out-of-process Python plugins (Tier 2) get the server's GPU.

### 6.2 Core vs plugin

| Core (in `arrix-*` crates) | First-party plugins (public API only; `plugins/` until C3, then their own repositories) | Third-party territory |
|---|---|---|
| Document, parameters and expressions, dependency DAG, undo, file format | Robotics: links, URDF/SDF/MJCF (plugin #1) | BIM, CAM, generative design, ML |
| Sketcher: solver, constraints, dimensions, trim/offset/mirror, projection | 2D drawings (section, projection, dimensions, PDF/DXF) | Vendor part catalogues |
| Part features: extrude, revolve, sweep, loft, boolean, fillet, chamfer, shell, draft, hole, patterns, mirror, split, move/copy body | Standard parts (fasteners, bearings) as a library plugin | Domain exporters |
| Reference geometry: datum planes, axes, points, frames | Sheet metal | Custom analyses |
| Assemblies: components, instances, joints/mates, FK | FEA pre-processing (mesh, boundary conditions, export) | |
| Materials (density, appearance), mass properties, measure, section view, interference check | Rendering and glTF scenes | |
| Exchange: STEP read and write with assembly structure, STL, OBJ, 3MF, DXF/SVG sketch in and out | | |
| Viewport, selection, picking; plugin host; CLI | | |

The rule for placing something new: **the core is what every mechanical user needs and what plugins build on. A domain is a plugin even when first-party.** When the answer is unclear, it starts as a plugin; promoting it to the core later is easier than demoting it.

A core feature exists only where Arris has the operation. Sweep, loft, shell, draft and interference wait for Arris cycles (§7). ArriX never works around a kernel gap in the application: it becomes an Arris fixture or an ask (§8).

### 6.3 The plugin model

**One interface, three tiers.** The plugin API is defined once, in WIT (the WebAssembly component model's IDL), and versioned by semver apart from the app (`arrix-plugin-api`). Each tier is a way of hosting the same interface:

| Tier | What | Runs | Trust | For |
|---|---|---|---|---|
| 0 | A Rust crate compiled into the binary | In process, native and wasm | Full (reviewed source) | First-party plugins |
| 1 | A WebAssembly component (`.wasm`) installed at runtime | wasmtime on native; not in the browser at first (a later ADR) | Sandboxed; capabilities in the manifest | Third-party Rust (and anything that compiles to a component) |
| 2 | A separate process speaking the interface over a wire protocol on stdio | Native only | Full: it is the user's Python | Python, GPU, numpy/torch, existing scientific code |

Tier 0 runs through the same trait objects the WIT bindings produce, so a first-party plugin is a proof that the public API is enough. The source never names a host-internal type; a layer lint enforces it.

**Contribution points.** A manifest (`arrix-plugin.toml`: id, version, API version range, tier, capabilities) declares what the plugin contributes:

1. **Features**: history entries with parameters, references and a deterministic `evaluate` that returns bodies (and datums, sketches or frames) with kernel provenance. Evaluated off the UI thread and memoised by content hash, like built-in features.
2. **Document data**: plugin-owned records in the document, namespaced by plugin id and versioned by the plugin's own schema, pointing at geometry by persistent reference (a link's bodies, an FEA load's faces).
3. **Commands**: undoable edits, composed from core commands plus the plugin's own document data.
4. **Importers and exporters**: file formats, as a file in and commands out, or a snapshot in and bytes out.
5. **Analyses**: read-only jobs over an evaluated snapshot, with progress and cancellation, returning reports or viewport overlays.
6. **UI**: **declarative only**, never raw egui. Ribbon groups, a property form (fields, lists, buttons, validation), a panel made of form widgets, a context-menu entry, and viewport drawables (points, lines, frames, labels, meshes with a colour field, gizmos with typed drag handles). The core renders all of them. Declarative UI crosses a process or wasm boundary, stays consistent, and is testable headless. Letting Tier 0 draw its own egui is an escape hatch to decide only once a real plugin needs it.

**No Arris types in the plugin API.** Geometry crosses as opaque handles, ArriX's own plain types (points, frames, profiles, meshes, persistent references) and, across a process boundary, Arris's body bytes (§8, ask A2). Kernel operations are a host service (`cx.kernel()`) with ArriX's signatures. Arris breaks its exhaustive enums every cycle by design, and the plugin ecosystem must not break with it.

**Determinism is part of the contract.** A feature's `evaluate` is a pure function of its parameters, its resolved inputs and the plugin's version. Memoisation, replay, frozen results and agent tests depend on this. A non-deterministic plugin is a bug, and the test kit checks for it by evaluating twice.

### 6.4 A document never needs its plugins to open
A plugin feature stores its **last result** with the document: the bodies as Arris body bytes, and the persistent names. Opened without the plugin (not installed, wrong version, a browser that can't run Tier 2), the feature is shown **frozen**: its result is there and downstream features evaluate against it, but its parameters are read-only and it says why. Plugin document data without its plugin is kept byte for byte and written back unchanged. For a FOSS format this is non-negotiable: a file must outlive the plugins that made it.

### 6.5 The document
* **Recipe as source of truth.** A document is parts, assemblies, parameters and plugin data. A part is a feature history; geometry is derived, cached, and always regenerable except in frozen features.
* **A dependency DAG from day 0.** It covers cross-feature and cross-part references, parameters named in expressions, and plugin features. Deferring it makes every cross-reference a special case.
* **Persistent names from Arris provenance.** Origin chains rooted at roles, with no geometric matching. A lost reference is a diagnostic with candidates, **never a silent rebind**.
* **File format:** `.arrx`, a zip whose unzipped form is a valid document directory. The recipe is deterministic, sorted JSON (diffable, agent-readable), with `blobs/` beside it (imported STEP, frozen results, thumbnails). The schema is versioned from v1 with forward migrations. Plugin sections carry their own versions.
* **SI internally** (metres, kilograms, radians), Z-up, right-handed; the UI shows mm/deg by default.

### 6.6 The kernel's sensor
ArriX is how Arris learns about real use, and this shapes the architecture from day 0, not a later feature:

* **Every kernel call is a replayable record.** `arrix-kernel` is the one choke point to Arris. Each call is described by a value (operation, operands, parameters, precision, Arris version) before it runs. With body bytes (§8.4), any call serialises as a self-contained Arris fixture recipe: the operands' closures, the operation and its arguments, and nothing else of the document.
* **A failed feature keeps its repro.** A kernel error on a feature leaves the failing call record beside the diagnostic, locally. "Save as kernel fixture" is a command, and in the CLI a flag.
* **Failures are countable.** Every failure maps to a stable category: the kernel's typed error, plus the feature type and the plugin that called it. The same failure from a thousand documents is one row with a count, which is what ranks the kernel's work.
* **Batch evaluation is a first-class API.** The CLI and the Python API evaluate many documents or recipes headless, sweep a parameter across a range, and emit outcomes as JSON (success or category; volume, area, counts). Dataset replay, agent users and continuity checks are built on it; none needs a special path.
* **Feature-history datasets arrive through ordinary importers** (§6.3, contribution point 4), so replaying one is an importer plus a batch run.
* **Nothing leaves the machine unasked.** Reporting a failure upstream is opt-in per report. The case is shrunk locally first, and the payload is shown exactly as it will be sent. Network code lives in one place, off by default, and a document's geometry is its author's.

Determinism (§6.3) is what makes all of this work: a record replays to the same failure on any machine, native or wasm.

### 6.7 Workspace layout
```
arrix-core          units, ids, plain math over glam, persistent-reference types
arrix-kernel        the only crate naming Arris types: ops, naming from provenance, render mesh, body bytes, call records
arrix-sketch        the constraint solver and sketch model (ported)
arrix-doc           document, DAG, features, commands, undo, evaluator, file format
arrix-plugin-api    the WIT world and its Rust traits and plain types (own semver)
arrix-plugin-host   tier 0/1/2 hosting, manifests, capabilities, frozen fallback
arrix-viewport      wgpu scene, ID-buffer picking, overlays (ported)
arrix-ui            egui widgets over plain view types; the declarative-UI renderer
arrix-app           the binary: glue, translation between doc and view types
arrix-cli           headless open / evaluate / export / run / test
plugins/robotics    first-party plugin #1 (Tier 0; in-tree until C3, then its own repository)
python/arrix        the Tier 2 SDK (PyPI), generated stubs plus a thin runtime
```
**Layer rules:** Arris only in `arrix-kernel`. egui/eframe/wgpu only in viewport, ui and app. `arrix-ui` never names a doc or sketch type. Plugins depend on `arrix-plugin-api` alone. A script checks all of these in the hook and in CI.

## 7. Roadmap Seed
A cycle retires the scariest remaining unknown first and ends in numbers. The first cycles are named here; `docs/ROADMAP.md` details them.

* **C1 — the vertical slice, with a plugin feature in it.** Workspace, gates and the headless harness first; the sketcher, viewport and snapshot harness ported. Then a document with a DAG; sketch → extrude → cut → fillet → pattern → hole; naming from provenance; save/load `.arrx` v1; STEP/STL/OBJ export. **And a Tier 0 plugin feature in the same history** (a spur gear), frozen when the plugin is removed. A failed kernel call saves as an Arris fixture recipe, and the CLI evaluates documents in batch (§6.6). The risk retired: *a plugin feature is a first-class history entry*. It has to be proven before anything is built on a document model that doesn't allow it.
* **C2 — the mechanical core.** Assemblies (components, instances, joints, FK; ported and generalised), materials, mass properties, measure, section view, STEP import as a feature with assembly structure (Arris ask A4), 3MF, DXF/SVG sketch exchange. Accept: a real multi-part assembly round-trips through STEP against another CAD's reading of it.
* **C3 — the boundary crossed.** Tier 2 (Python) and Tier 1 (wasm component) hosts; robotics moved out as plugin #1; a second first-party plugin in Python (standard fasteners, or FEA export) through Tier 2 only; the plugin test kit; `arrix-plugin-api` reviewed for 0.x stability. Accept: the north-star demo.
* **C4 — the web.** The browser build with a worker executor; Tier 0 plugins only. A document using Tier 1 or Tier 2 plugins opens in the browser with those features frozen (§6.4). Tier 1 in the browser is a later decision, with its own ADR.
* **Named, unordered:** sweep/loft/shell/draft features (gated on Arris's sweep cycle), interference checks (Arris's query cycle), a drawings plugin, sheet metal, a general assembly mate solver, configurations, a plugin index, **remote mode** (a self-hosted server with thin native and browser clients, §6.1: authentication, presence, transport; branching and merging documents as a separate question).

**North-star demo:** sketch a bracket → extrude, fillet, pattern holes → import a vendor servo from STEP and mate it → a **Python** plugin places fasteners from a library into the picked holes → the **robotics** plugin exports URDF that loads in RViz → save, open the file on a machine with neither plugin installed: every body is there, the plugin features are frozen and say why, and editing the bracket's width still updates everything downstream of them.

## 8. Heritage and the Arris Contract

### 8.1 Ported code (reviewed on the way in)
* The **sketch solver** (LM/DogLeg, analytic Jacobian, DoF and redundancy diagnostics) and the sketch model, trim/extend/offset/mirror included. It is the largest asset and depends on no kernel.
* The **viewport**: per-body `Scene`, ID-buffer picking, outline pass, ViewCube, camera.
* The **headless visual harness**: `egui_kittest` snapshots, `debug_state()` JSON, the scratch render, the `visual-debug` skill.
* The **workflow**: `/idea` `/plan` `/work` `/retire-plan` `/close-cycle`, the git and docs-lifecycle rules, size and wasm lints, the hooks.

Redesigned rather than ported: the document model (plugin features, DAG, frozen results), the kernel boundary (no swap facade; the plugin API is the stable boundary now), and the app shell (contribution points).

### 8.2 Principles learned the hard way
1. **One gesture = one command.** A field commits on focus loss and a drag on release; a commit equal to the current state produces no command.
2. **What a snap shows is what it commits.** An inference preview and the constraint written are the same object.
3. **Fail soft, never silently.** A failed feature keeps its parameters and says why, and the rest of the model lives. No tolerance nudge; no silent re-bind.
4. **The UI never blocks; the kernel never runs on the UI thread.**
5. **SI inside, Z-up, right-handed.**
6. **The agent sees the UI itself.** Headless snapshots and a JSON of what was drawn; a golden is never refreshed without reading its diff.
7. **Scope is argued, not accreted.** Work outside the current cycle names the scenario it unlocks first; otherwise a sketcher grows constraint kinds no tool can reach.
8. **Split, don't allowlist**: 1,500 lines per file, 200 per function.

### 8.3 The Arris contract
Arris is a crates.io dependency with pinned minors, and `[patch.crates-io]` only for an unreleased fix. A gap found here becomes an Arris regression fixture or backlog line, never an `#[ignore]` or workaround here. ArriX is Arris's first consumer: its side-by-side regressions rank Arris's next cycle (Arris ADR-0020). ArriX does not need Arris's API to be stable; its own plugin API is the stable layer.

### 8.4 What ArriX asks of Arris
The kernel changes this design needs are recorded in Arris's repository as the idea `docs/ideas/arrix-consumer-asks.md`. Four block C1: consumer-defined provenance roles (a plugin's own topology gets stable names), body bytes with a compatibility policy (frozen results, the Python process boundary), cancellation of long operations, and `ops::mirror`. Later ones are STEP product structure, a public 2D arrangement, multi-tool booleans, per-face tessellation, the query, sweep and healing cycles, and the Python binding. The attribute cycle, API stability before 1.0 and IGES are not needed. Arris decides and schedules them under its own rules.

## 9. Decisions taken in this seed

| Question | Decision | Reason |
|---|---|---|
| Clean start or continue earlier code? | **Clean repository; port what holds, distil the rest into principles** | Plugin features and a DAG redesign the document core anyway; earlier architecture was shaped by kernels no longer used; agents read less stale context |
| General or domain CAD? | **General core, domains as plugins, first-party ones included** | A FOSS CAD lives by its ecosystem; a domain in the core is a boundary lost |
| Plugin languages | **Rust and Python** (plus anything that compiles to a wasm component) | Rust for the ecosystem's native plugins; Python for science, GPU and existing code |
| Plugin interface | **One WIT world, three hosting tiers** | One definition; Tier 1 comes free from the component model, Tier 2 serialises the same types |
| Plugin UI | **Declarative, rendered by the core** | Crosses process and wasm boundaries; consistent; headless-testable |
| Kernel types in the plugin API | **None** | Arris's API breaks by design; the ecosystem must not |
| Plugin-made documents without the plugin | **Open, with frozen results** | A file must outlive its plugins |
| Dependency DAG | **Day 0** | Plugin features and cross-references need it; deferring it makes every cross-reference a special case |
| File format | **Zip of a deterministic JSON directory, versioned, with migrations** | Diffable, agent-readable, one file to share |
| Kernel | **Arris, no swap facade; Arris types confined to `arrix-kernel`** | The kernel is ours; the confinement protects the plugin API, not swapability |
| UI stack | **egui/eframe/wgpu** (ported) | Pure Rust, native and web, immediate mode fits a projection of the document |
| Licence | **MIT OR Apache-2.0** | Matches Arris; plugins may be under any licence, commercial ones included. Copyleft would deter plugin vendors, and the Tier 0 derived-work question would be murky |
| Home | **A GitHub organisation for the family, chosen later; `Divelix/arrix` until then** (ADR-0002): kernel, app and plugins as separate repositories | One family, visible together; `arris` and `arrix` are taken; the name should not be narrower than what plugins will make of it |
| First-party plugins | **In-tree until C3, then their own repositories** | A breaking API change is one commit while the API is young; moving out proves the out-of-tree path third parties use |
| Plugins on the web | **Tier 0 only at first** | Hosting components inside a wasm app is unsolved in the ecosystem; frozen results cover documents that use other tiers |
| First cycle | **Vertical slice including a plugin feature** | Proves the document model admits plugins before anything is built on it |
| Server and thin clients | **Not built soon; not precluded: a serialisable command/event boundary, per-author undo, generation-checked commands, per-client session state, local = in-process transport** | Cheap as discipline from day 0; retrofitting it into a desktop CAD is a rewrite |
| Kernel failures | **Replayable records from day 0; reporting opt-in, shrunk and previewed** | A young kernel's corpus has to grow from real use; retrofitting a choke point and a record format is harder than starting with one |

## 10. Directives for the AI Agent
With this seed agreed, and before writing application code:
1. **Scaffold the repository and the docs system**: `AGENTS.md`/`CLAUDE.md` with a current state of ≤15 lines, `docs/README.md` with the lifecycle, `docs/adr/`, `docs/ideas/`, `docs/plans/`, `BACKLOG.md`, the `.agents/rules` (git and docs lifecycle) and skills, hooks, and size, wasm and layer lints.
2. **Design documents**, present tense, one per topic: `ARCHITECTURE.md` (crates, layer rules, the data flow, threading), `DATA-MODEL.md` (document, DAG, features, naming, file format, frozen results), `PLUGINS.md` (the WIT world, tiers, manifest, contribution points, capabilities, versioning, the test kit), `UI-RENDERING.md`, `CONCURRENCY-WASM.md`, `ROADMAP.md` (C1 in full, the rest as one line each).
3. **Port the harness before features**: the snapshot harness, `debug_state()` and the CLI's evaluate-and-dump come first, so every later step is checkable.
4. **Track the Arris asks** (§8.4): an ArriX step blocked on one names it, and never works around it.
5. **Write an ADR for each `⚠ OPEN` when it closes.** Open at kickoff, both technical and both the agent's to propose: undo granularity for plugin commands (C1; within §6.1's per-author inverse-command rule), and the Tier 2 wire encoding, JSON-RPC or a binary encoding of the WIT types (C3). Settled default, to be written into `DATA-MODEL.md`: when an upstream edit changes a frozen feature's inputs, it keeps its old result, is marked stale, and is re-frozen the next time its plugin runs.
