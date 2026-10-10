# Roadmap

A cycle retires the scariest remaining unknown first and ends in numbers
(`SEED.md` §7). Its "out" list is as binding as its "in" list: work outside
the open cycle's in-list needs an ADR naming the product scenario it unlocks
before its plan starts. One section per cycle; a finished cycle compresses
to its status line (`/close-cycle`), and the next is detailed below it.

Spine: **C1** (the vertical slice with a plugin feature in it; done), then
**the boundary opened** (the agent's way in; ADR-0016; done), **C2** (the
mechanical core; next), **C3** (the boundary crossed), **C4** (the web). Beyond
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

## C2 — the mechanical core

Assemblies (components, instances, joints, FK), materials, mass
properties, measure, section view, STEP import as a feature with assembly
structure (A4), 3MF, DXF/SVG sketch exchange. First, the exchange unit:
Arris's STEP writer declares millimetres for ArriX's metres, so a length
unit on the writer (an Arris ask) and an ADR fixing ArriX's exchange unit
come before import. Each capability says core or plugin when it is
planned (`SEED.md` §6.2). Accept: a real multi-part assembly round-trips
through STEP against another CAD's reading of it.

## C3 — the boundary crossed

The Tier 1 (wasm component) host; `arrix-plugin-test`, the Rust kit for
Tier 0 authors, beside the Tier 1 SDK; robotics moved out as plugin #1; a
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
