# Roadmap

A cycle retires the scariest remaining unknown first and ends in numbers
(`SEED.md` §7). Its "out" list is as binding as its "in" list: work outside
the open cycle's in-list needs an ADR naming the product scenario it unlocks
before its plan starts. One section per cycle; a finished cycle compresses
to its status line (`/close-cycle`), and the next is detailed below it.

Spine: **C1** (the vertical slice with a plugin feature in it; done), then
**the boundary opened** (the agent's way in; ADR-0016; done), **C2** (assemblies
and exchange; done), **C3** (the boundary crossed; done), **C4** (the web). Beyond
C4, the named cycles are unordered.

---

## C1 — the vertical slice, with a plugin feature in it

*Goal: a part is sketched, extruded, cut, filleted, patterned and holed,
with a spur gear from a Tier 0 plugin in the same history, and everything
downstream of the gear follows an upstream edit. Saved and reopened without
the plugin, every body is there, the gear is frozen and says why, and the
features after it still evaluate.*

**Status: done 2026-10-08, tag `c1`.** The risk retired: a plugin feature is
a first-class history entry (M1's accept, `crates/arrix-cli/tests/slice.rs`,
before any other feature existed), and names hold through booleans, fillets
and patterns, lost references ending in diagnostics (M2). M0 to M4 landed
2026-09-26 to 10-08 on `arris` 0.4 and 0.5; the plans are in the git log and
the decisions in ADR-0004 to ADR-0020. One budget is missed, the first
coarse mesh at 324 ms against 150 (`docs/CONCURRENCY-WASM.md` §Budgets, an
Arris finding).

- M0, the gate and the harness: the workspace of `docs/ARCHITECTURE.md`
  §Crates, the hooks and lints, the visual harness, `arrix eval` on an
  empty document.
- M1, the document and a plugin feature in its history: `arrix-sketch`
  ported, `arrix-doc` with commands, per-author undo, `LocalSession` and
  the evaluator, `core.sketch`, `core.datum-plane`, `core.extrude`, minimal
  `core.boolean`, plugin API 0.1 to 0.3 with Tier 0, `gears.spur`, frozen
  results, `.arrx` v1.
- M2, part features and naming: `core.revolve`, `fillet`, `chamfer`,
  `hole`, both patterns, `mirror`, `boolean` in full; naming through every
  one; lost references as diagnostics; the kernel's sensor and fixture
  export; projection into a sketch.
- M3, the interactive app: the viewport with ID-buffer picking, the shell
  configured by user files, sketch mode, forms and previews, one gesture =
  one command, checked.
- M4, exchange and batch: STEP, STL and OBJ export, `arrix eval` complete,
  the C1 benchmark, cancellation inside a kernel call.

**Out:** assemblies and joints; materials, measure and section view; STEP,
3MF and DXF/SVG import (C2); the Tier 2 host, `arrix serve` and the Python
SDK (the boundary opened); the Tier 1 host (C3); the browser running the
app (C4); sweep, loft, shell and draft; the robotics plugin (by C3); network
code of any kind, upstream failure reporting included; parallel evaluation
inside one document; a Tier 0 plugin drawing its own egui.

**Accept:** one scenario (the bracket, `tests/docs/bracket`), run by `cargo
test -p arrix-cli --test c1_accept` and by `arrix eval`: built through the
commands the UI issues; the plate's width edited with no reference lost;
saved twice to identical bytes, undo and redo byte for byte; opened without
the plugin (the gear frozen with its reason, stale after a width edit,
re-frozen on return); an oversize fillet failing with its category and a
saved recipe that reproduces it in Arris; STEP, STL and OBJ read back;
`arrix eval --sweep` over 50 widths with the failure histogram; the M3
snapshot scenarios, the budgets and the whole gate green.

## The boundary opened

*Goal: a user's agent, without ArriX's source, extends and drives a running
ArriX: it writes a feature plugin in Python, loads it into a session, builds
a document over a stdio protocol, renders it headless and checks the result.*

**Status: done 2026-10-11, tag `c1b`.** The risk retired: the `Session`
protocol and the plugin world hold across a process boundary. A Python
feature runs as Tier 2 in a real history and is frozen without its plugin
(M1), `arrix serve --stdio` carries the session and loads a plugin by a
request (M2), and an agent with only the binaries and the SDK does the whole
scenario (M3, `cargo test -p arrix-cli --test boundary_m3`). M1 to M3
landed 2026-10-09 to 10-11 on `arris` 0.6; the plans are in the git log and
the decisions in ADR-0021 to ADR-0026.

- `arrix serve --stdio`: the `Session` protocol framed on stdio, JSON-RPC,
  the wire generated from the WIT world (ADR-0022, ADR-0023).
- The Tier 2 host in `arrix-plugin-host`: one process per plugin, capabilities
  from the manifest, a crash or timeout as that feature's diagnostic with
  its result frozen.
- The Python SDK `python/arrix`: stubs generated from the WIT world, a thin
  runtime, pure stdlib, geometry through the host's kernel by callback
  (ADR-0021), and `arrix.session` and `arrix.eval` for an agent.
- The test kit packaged (`arrix test`): determinism, round trip, frozen
  open, migration, spec and kernel failure, for a plugin outside the
  repository (`docs/PLUGINS.md` §The test kit).
- The harness as product (ADR-0026): the eval line, the render JSON, the
  kit's report and the protocol each versioned, with schemas held by goldens.

**Out:** assemblies, STEP import and the mechanical core (C2); the Tier 1
host and robotics (C3); the browser (C4); remote mode, authentication and
any transport beyond stdio; distributions; a plugin index; the Tier 2
plugin drawing its own UI beyond the declarative forms.

**Accept:** ADR-0016's scenario, run by `cargo test` from a clean directory
holding only the installed `arrix` and the SDK: an agent-written Python
feature plugin is loaded into `arrix serve --stdio`; a document is built
over the protocol, evaluated, rendered headless and checked by its JSON; the
document opens on a machine without the plugin with the feature frozen and
its reason given; the packaged test kit passes the plugin; the whole gate
stays green.

## C2 — assemblies and exchange

*Goal: a document holds assemblies of parts, imported vendor parts among
them, placed by joints whose values are expressions, and an assembly leaves
and returns through STEP with its structure, in metres.*

**Status: done 2026-10-13, tag `c2`.** The risk retired: a persistent name
across parts and instances, into imported and frozen geometry. M1 (assemblies
in the document, headless; `--test c2_m1`), M2 (the app: instanced meshes,
picks by component path, the joint drag; `c2_m2_accept`) and M3 (STEP with
product structure both ways in metres, materials, mass; `--test c2_accept`,
held to Open CASCADE's reading) landed 2026-10-11 to 10-13 on `arris` 0.6 to
0.8; decisions in ADR-0027 to ADR-0033.

**In:**
- The exchange unit (ADR-0028): metres in, metres out; STEP export
  declares metres (A12, `arris` 0.8).
- `core.import-step`: a feature whose file is a blob in the document and
  whose faces are named from the file's entities.
- Assemblies (ADR-0029): components, nested assemblies, fixed, revolute and
  prismatic joints, tree forward kinematics, schema 4.
- Materials (density, appearance) and mass properties per body, part and
  assembly.
- STEP with product structure both ways (A4).

**Out:** measure, section view, 3MF and DXF/SVG exchange (the named cycle
"the mechanical core completed", after Arris's query cycle, A6); a general
mate solver and closed loops; interference checks; in-context design (a
feature naming another part's geometry); references to other `.arrx` files;
healing a vendor file (A7).

**Accept:** a real multi-part assembly round-trips through STEP against
another CAD's reading of it: Open CASCADE through a script, never in CI,
into a committed text golden, on an assembly ArriX builds (bracket, servo,
horn) and one vendor STEP Arris reads cleanly.

## C3 — the boundary crossed

*Goal: the north-star demo (`SEED.md` §7) runs. A Python plugin places
library screws into a bracket's picked holes as components, a Rust plugin
hosted as a wasm component exports URDF that another reader loads, and the
file opens with neither plugin, frozen, still following a thickness or spacing edit.*

**Status: done 2026-10-14, tag `c3`.** The risk retired: the plugin API
holds across every boundary. A Rust plugin runs sandboxed as a component
exactly as it runs compiled in, and two real domains need nothing but the
public API. M1 (Tier 1; `gears` identical at Tiers 0 and 1), M2 (the snapshot
read, exporters, `robotics`) and M3 (commands, `fasteners`, out of tree, the
API review, the accept: `--test c3_accept`, `--test visual c3_accept`) landed
2026-10-13 to 10-14, API 0.8.0; decisions in ADR-0034 to ADR-0039.

**In:**
- The Tier 1 host (wasmtime, component model, native only; fuel and an
  epoch interrupt) and the Rust guest SDK, a feature of `arrix-plugin-api`.
- A snapshot read (`document-read`) and the `exchange` export, on all three
  tiers; `arrix export --exporter <id>`.
- `robotics`, plugin #1: Rust, a Tier 1 component, URDF of an assembly.
- The `commands` export: a gesture expanded into one `Group` (ADR-0004),
  bound to a ribbon button and the selection.
- `fasteners`, plugin #2: Python, Tier 2 only; ISO 4762 screws as
  components held by a fixed joint on a picked hole's edge.
- Out of tree, proven in tree: each first-party plugin a Cargo workspace
  of its own on `arrix-plugin-api` alone, passing `arrix test` from the
  installed binary; `arrix-plugin-test`, the Rust kit.
- `arrix-plugin-api` reviewed for 0.x stability, by ADR.

**Out:** Tier 1 in the browser (C4's ADR); SDF and MJCF; the plugins'
own repositories and a published `arrix-plugin-api` (the human's acts,
after the accept); plugin document data, importers, analyses, panels and
viewport drawables by plugins; unloading a plugin; FEA export; screw
families beyond one, and modelled threads; a plugin index; distributions.

**Accept:** the north-star demo, run two ways: headless through `arrix
serve` with the picks as references, and in the app, the fastener command
run from the ribbon on picked hole edges. The URDF is held to urdfdom's and a
Python loader's reading by a script, never in CI, into a committed text
golden. Saved and opened with neither plugin, the fastener features are
frozen with their reason, and an edit of the plate's thickness or of the
pattern's spacing moves the holes and the screws.

## C4 — the web

*Goal: the app runs in a browser tab. A document opens from bytes, edits
evaluate in a worker, and one that uses a Tier 1 or Tier 2 plugin opens with
those features frozen, with their reason (`SEED.md` §7).*

**Status: done 2026-10-23** (ADR-0042; accept: `cargo nextest run -p arrix-app
--test c4_accept`). The risk to retire: the
native evaluator and its determinism hold on wasm32 in a worker. The
authority runs on the main thread and the evaluator alone in a worker; a
commit stops a superseded evaluation through a `SharedArrayBuffer` counter
on a cross-origin-isolated page, within 50 ms.

**In:**
- The wasm32 app build (eframe on the web), Tier 0 plugins only, no
  `PluginLoader` (`LoadPlugin` and `ReloadPlugin` rejected, ADR-0024).
- A worker executor behind the existing executor seam, with the
  `SharedArrayBuffer` cancellation of ADR-0042 and its no-isolation fallback.
- Files as bytes through the browser's file APIs: open, import, save and
  export as downloads.
- The frozen fallback shown in the page for Tier 1 and Tier 2 features.

**Out:** Tier 1 in the browser (its own ADR); Tier 2 (a process); remote mode
and accounts; offline storage beyond a download; touch and mobile layouts.

**Accept:** in a headless browser, `tests/docs/bracket` opens, a width edit
evaluates in the worker, and its eval line equals native's, to the bit; an edit
made during an evaluation stops it within 50 ms; a feature of a plugin the page
lacks opens frozen, bodies kept, with its reason shown; a document saved as a
download evaluates natively to native's eval line.

---

## Named cycles, unordered

None is scheduled. Each earns a section when `/close-cycle` opens it.

- Sweep, loft, shell and draft features (on Arris's sweep cycle).
- Interference checks (on Arris's query cycle).
- A drawings plugin.
- Sheet metal.
- The mechanical core completed: measure, section view, 3MF, DXF/SVG
  sketch exchange (after Arris's query cycle, ADR-0027).
- A general assembly mate solver.
- Configurations.
- A plugin index.
- Distributions: a named, shareable composition of plugins, config and
  ribbon layout; after the runtime tier (ADR-0016).
- Remote mode: a self-hosted server with thin native and browser clients
  (`SEED.md` §6.1): authentication, presence, transport. Branching and
  merging documents is a separate question.
