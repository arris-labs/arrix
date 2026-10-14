# Plugins

The plugin model is ArriX's reason to exist (`SEED.md` §3, §6.3): one
interface, versioned apart from the app, hosted three ways, and enough of it
that first-party domains need nothing else. This document is the design of
`arrix-plugin-api` and `arrix-plugin-host`. Built, as of C3 M2: the world and
its Rust traits at 0.7.0 (§One interface), the Rust guest SDK (`guest`
feature, `export_plugin!`), manifests (§The manifest), Tier 0, Tier 1 and
Tier 2 hosting (§Three tiers), the test kit (§The test kit, `arrix test`),
the snapshot read and exporters on all three tiers (§4), and the first
plugins, `plugins/gears` with `gears.spur` and `plugins/robotics` with
`robotics.urdf` (their crate docs state their geometry and conventions), at
Tier 0 or as Tier 1 components.
`arrix-plugin-test` comes with C3 M3 (ADR-0034).

## One interface, defined once

The interface is a WIT world (the WebAssembly component model's IDL),
`wit/arrix-plugin.wit` in `arrix-plugin-api`, package `arrix:plugin` at the
API's version. From it come:

- the **Rust traits and plain types** of `arrix-plugin-api`, which Tier 0
  plugins implement directly and Tier 1 plugins implement through the
  `guest` feature's bindings (`export_plugin!`, ADR-0035);
- the **host side** in `arrix-plugin-host`: an adapter per tier that
  presents a plugin, however hosted, as the same trait objects;
- the **Python SDK** (`python/arrix`): stubs generated from the world, and
  a thin runtime speaking the Tier 2 wire protocol.

The world as it stands at 0.7.0 (the file is the authority; comments and
field lists are elided here; `document` and `exchange` follow it):

```wit
package arrix:plugin@0.7.0;

interface types {                                    // SI; no Arris type anywhere
  type id = u64;                                     // arrix-core's typed ids
  record vec2, vec3, frame { origin, x-axis, z-axis }
  enum quantity-kind { length, angle, count, ratio, mass, density }
  record quantity { kind, si: f64 }
  record persistent-ref { encoded: string }          // a PersistentName's text form, opaque here
  variant reference { param, feature, slot, topo, sketch, region, plugin }   // arrix-core's Ref
  record region-ref { feature, entities: list<id>, sample: tuple<s64, s64> } // a RegionKey, nm
  record diagnostic { severity, code, message, refs: list<reference>, candidates: list<reference> }
  record profile { plane: frame, outer: profile-loop, holes: list<profile-loop> }
  // profile-loop: a keyed circle, or a path of keyed line and arc segments
  record mass-properties { volume, area, centroid }
  record axis { origin, direction }                  // a revolve's axis; an `axis` input
  record point { at, normal: option<vec3> }          // a `point` input
  record motion { translation, axis, angle }         // a rigid motion, for `copy`
}

interface kernel {                                   // a host service; imported by the plugin
  resource body;                                     // an opaque handle, valid within one evaluation
  extrude: func(p: profile, distance: f64) -> result<body, diagnostic>;
  revolve: func(p: profile, a: axis, angle: f64) -> result<body, diagnostic>;
  names: func(b: borrow<body>) -> result<list<persistent-ref>, diagnostic>;
  face-frame: func(b: borrow<body>, face: persistent-ref) -> result<frame, diagnostic>;
  measure: func(b: borrow<body>) -> result<mass-properties, diagnostic>;
  fuse: func(target: borrow<body>, tool: borrow<body>) -> result<body, diagnostic>;
  cut: func(target: borrow<body>, tool: borrow<body>) -> result<body, diagnostic>;
  common: func(target: borrow<body>, tool: borrow<body>) -> result<body, diagnostic>;
  fillet: func(b: borrow<body>, edges: list<persistent-ref>, radius: f64) -> result<body, diagnostic>;
  chamfer: func(b: borrow<body>, edges: list<persistent-ref>, distance: f64) -> result<body, diagnostic>;
  copy: func(index: u32, b: borrow<body>, m: motion) -> result<body, diagnostic>;
  mirror: func(b: borrow<body>, origin: vec3, normal: vec3) -> result<body, diagnostic>;
}

interface feature {                                  // exported by a plugin that adds features
  describe: func() -> list<feature-type-spec>;      // id, version, title, params, choices, inputs, output slots
  // input-kind { plane, region, body, edge, point, axis, tool }; input-spec { name, title, kind, list, shown-when }
  // param-spec { name, title, kind, default, shown-when }; shown-when { choice, values }
  // choice-spec { name, title, options: list<choice-option>, default: option<string> }
  // slot-spec { name, kind, modifies: option<string> }; record tool { body, op: tool-op }
  // input-value { plane(frame), region(profile), body(body), edge(persistent-ref), point(point), axis(axis), tool(tool) }
  // feature-output { slots, tool: option<tool> }
  evaluate: func(type-id: string, params: list<param-value>, choices: list<choice-value>,
                 inputs: list<resolved-input>)
            -> result<feature-output, diagnostic>;
}

world plugin {
  import kernel;
  import document;
  export feature;
  export exchange;
}
```

- **Parameters** arrive evaluated: the host evaluates each field's
  expression to a `quantity` in SI; a `param-spec`'s default is an
  expression's text (`1 mm`).
- **Inputs** arrive resolved: a `plane` input (a datum, a planar face or
  a world plane) is a `frame`, and a `region` input (a sketch region, by
  `Ref::Region`) is its `profile` on the sketch's plane, keyed by the
  entities that bound it. Output slots hold a `body` or a `plane`; a
  sketch slot is a built-in's alone (`docs/DATA-MODEL.md` §Features).
- **Choices and where fields show** (ADR-0018, 0.5.0): a type declares
  `choices`, each a name, a title, its options (a word and a title) and
  a default, which is none for a choice whose being unset means something
  (a datum plane's `world`). The host gives `evaluate` every choice that
  has a value, the record's or the default (`choice-value`), refuses a
  name the type does not declare as `feature.unknown-field` and a word it
  does not list as `feature.choice`, and hashes the record's choices as
  written. A `param-spec` or `input-spec` may be `shown-when` a choice
  holds one of some values: that is the form's, and a hidden field is
  neither shown nor required, its value kept. A form is derived from the
  spec alone (`docs/UI-RENDERING.md` §Declarative plugin UI).
- **Bodies as inputs** (ADR-0007, 0.3.0): an input of kind `body` arrives
  as a `body` handle of the feature's own to its slot's version current at
  the feature (an owned handle, not a borrow, ADR-0008), and
  an output slot spec may name the body input it `modifies`, making its
  body that slot's next version rather than a slot of its own. A body
  input no slot modifies is a tool, and its slot ends there. The kernel
  service's `fuse` and `cut` (target, tool) are how a feature joins or
  cuts, a built-in's or a plugin's alike
  (`docs/DATA-MODEL.md` §Bodies across features).
- **List inputs and the new kinds** (ADR-0012, 0.4.0): an `input-spec`
  says whether it takes a list (`list`). A list input arrives as one
  `resolved-input` per element, in the record's order, each with the
  input's name (`FeatureArgs::inputs_named`), and a list on a single
  input, or one reference on a list input, is `input.list`. An `edge`
  input is the edge's persistent name (a lost one is `ref.lost` with
  candidates; which body it is on is the kernel call's to check,
  `kernel.unknown-name`), a `point` a sketch point or a vertex in world
  SI (a sketch point with its plane's normal), an `axis` a sketch line or
  a straight edge (a unit direction; a curved edge is `input.wrong-kind`),
  a `tool` the tool a joining or cutting feature kept.
- **The tool a feature keeps** (ADR-0013, 0.4.0): a feature output may
  carry `tool`, the body it applied and `fuse` or `cut`; the evaluator
  holds it beside the feature's slots, no part body and never consumed,
  and only a `tool` input (a `Ref::Feature`) reads it. `core.extrude` in
  `join` and `cut` modes keeps its tool. A frozen result stores none, so
  a pattern of a plugin feature needs the plugin present
  (`input.no-tool`).
- **Booleans and blends are numbered by the host** (ADR-0014): `fuse`,
  `cut`, `common`, `fillet` and `chamfer` take no number in the API; the
  host counts a feature's calls, so a plugin that makes several keeps
  distinct names by making them in a stable order. `revolve` is counted
  with `extrude` (ADR-0011), and `copy` takes its instance index from the
  caller, since a pattern's count must not rename a copy that stays
  (ADR-0010).
- **Profiles are plain records**, keyed curve by curve like
  `arrix_core::Profile`, since a key is what roots a side face's name.
- **What grows it, by minor version** (breaking while 0.x): `migrate`
  with a feature type's second `type_version`; primitives, `shell`,
  `sweep`, `loft` and body bytes (ask A2) as the kernel service gains
  them; `import host` (log, progress, cancellation check,
  snapshot reads) and the `commands`, `exchange`, `analysis` and `ui`
  exports as the first plugin needs each.

**The snapshot read and exporters** (ADR-0037, 0.7.0): the world gains `interface document` and `interface exchange`,
one world, so a plugin implements what it contributes and the SDK supplies
the rest empty. `world plugin { import kernel; import document; export
feature; export exchange; }`.

- `document` is read-only and granted by `document-read`. By function:
  the parts (final bodies, material name and colour), each body's refined
  mesh (the refined chord, 1e-4 of the body's box diagonal; `f32` positions and normals, `u32` triangles, in the
  part's frame), each part's mass, centroid and inertia or its `mass.*`
  diagnostic, and the assemblies: components at every depth by path with
  their solved poses, and joints (kind, parent and child paths, anchors,
  axis, limits, value, status). It may be called only during an
  `exchange` call, against that call's generation; any other call is
  `plugin.capability`.
- `exchange` has `exporters()` (id `<plugin>.<name>`, title, extension,
  subject part or assembly, `params` and `choices` as a feature's specs)
  and `export(id, subject, params, choices) -> result<list<file>,
  diagnostic>`; `file { name, bytes }`, the first the main one. A name is
  relative, `/`-separated, without `..`: otherwise `export.path`.
- `export_plugin!(features = A, exporters = B)` names either or both;
  `export_plugin!(A)` is features only.

**The Rust side mirrors it**: `Kernel` and `Feature` traits and plain
types, with `arrix-core`'s own types (`Frame`, `Profile`,
`PersistentName`, `Ref`, `Diagnostic`, `Quantity`) re-exported where the
world's type is theirs, so a plugin names them through
`arrix-plugin-api` and the host passes them unconverted. `Feature::
evaluate` takes the kernel as `&mut dyn Kernel`, where the world imports
it. `crates/arrix-plugin-api/tests/wit_equality.rs` holds the two equal
three ways: every world type converts to its Rust type and back with
exhaustive destructuring on both sides, and round trips; `feature`'s
generated `Guest` is implemented over a Rust `Feature` and the reverse
(the `guest` feature's `Exported`, which Tier 1 plugins ship), and the
Rust `Kernel` over the generated imports (its `WitKernel`), so a function added on
either side fails to compile; and the world as `wit-parser` reads it is
exactly the inventory the test covers, so an addition the bindings would
silently absorb fails too.

**No Arris types in the plugin API** (`SEED.md` §6.3). Geometry crosses as
opaque handles (`body`), ArriX's plain types (points, frames,
profiles, meshes, persistent references), and as handles across a
process boundary too (ADR-0021). Kernel operations are a host service with
ArriX's signatures, implemented in `arrix-kernel`, recorded like every
other kernel call (`docs/ARCHITECTURE.md` §The kernel choke point). Arris
breaks its enums every cycle by design; the plugin API absorbs that in one
crate and does not pass it on.

**No host-internal types either.** A plugin names only `arrix-plugin-api`.
The layer lint fails a plugin crate with any other `arrix-*` dependency
(`docs/ARCHITECTURE.md` §Gates). A need the API cannot meet is an API
change, reviewed like any other, never a private hook.

## Three tiers

| Tier | What | Hosted by | Runs on | Trust | Cycle |
|---|---|---|---|---|---|
| 0 | a Rust crate compiled into the binary | direct calls through the API's traits | native and wasm | full: reviewed source | C1 |
| 1 | a WebAssembly component (`.wasm`) installed at runtime | `wasmtime`, with the component model | native; the browser later, by ADR | sandboxed; capabilities enforced | C3 |
| 2 | a separate process speaking the interface over stdio | a child process and a wire codec | native only | full: it is the user's own Python | the boundary opened |

- **Tier 0 goes through the same trait objects** the Tier 1 bindings
  produce. A first-party plugin that works at Tier 0 is therefore proof the
  public API is enough; one that needs more changes the API.
- **Tier 0 registration is an explicit list** in each binary that
  evaluates: `arrix-cli` (`crates/arrix-cli/src/plugins.rs`: `register_tier0(&mut
  registry, arrix_gears::MANIFEST, Arc::new(arrix_gears::Gears))`), not
  linker tricks, which do not work on wasm. A plugin crate exports its
  manifest's text (`MANIFEST`, `include_str!` of its
  `arrix-plugin.toml`) and its `Feature`; `arrix-app` gets its list when
  it first evaluates (`crates/arrix-app/src/commands.rs`). A build without a plugin is a build that
  leaves it off the list, and the CLI can also disable a compiled-in
  plugin for a run (`--without-plugin <id>`, with frozen results), which
  is how the frozen path is tested.
- **Tier 0 as built** (`arrix_plugin_host::register_tier0`): the
  manifest must say tier 0 and an `api` range holding this host's
  `API_VERSION`; every type the plugin describes must be
  `<plugin-id>.<name>`; all its types register or none do. Each is
  adapted onto `arrix_doc::FeatureType` through the plugin API's traits
  only, with the manifest's `version` as its plugin version (in every
  input hash). A plugin type takes the choices it declares. A panic in `evaluate` is
  caught and becomes the feature's `plugin.panic` diagnostic, and the
  evaluator carries on; natively only, since a wasm32 build aborts on a
  panic. A panic inside a kernel call it made is caught the same way, but
  the model's state after one is not vouched for.
- **Tier 1** (ADR-0035; built in C3 M1) runs a component in
  `wasmtime` with the component model, native only. A Rust author depends
  on `arrix-plugin-api` alone, enables its `guest` feature, writes
  `arrix_plugin_api::export_plugin!(MyFeature)` under `forbid(unsafe_code)`
  and builds a `cdylib` for `wasm32-unknown-unknown`; the host
  componentises it at load (`wit-component`). The manifest's `[tier1]`
  table names the `.wasm` and may raise the fuel. The sandbox:
  - the component's imports are the world's alone; any other is refused
    at load (`plugin.load`);
  - each evaluation gets a fresh store from one pre-instantiated
    component, so nothing survives between evaluations;
  - fuel is the deterministic step budget (`plugin.fuel`, default 2^30,
    at most 2^36), memory is capped at 256 MiB (`plugin.memory`), and a
    trap or a guest panic is `plugin.trap`, each ending only that feature;
  - a superseded evaluation stops the component through an epoch deadline
    callback that polls the evaluator's interrupt flag
    (`kernel.interrupted`).
  A plugin gives the same eval line, kernel calls and saved bytes at Tier
  0 and Tier 1, to the bit, its math being the API's (ADR-0036;
  `crates/arrix-plugin-host/tests/tier1_parity.rs` holds `gears` to it).
  An exporter is the same: `exchange.run` runs in a fresh store with fuel
  and the epoch interrupt, its `document` calls served by the host from the
  export's snapshot (and refused as `plugin.capability` in an evaluation or
  outside a call); a component importing `document` without `document-read`
  is refused at load. `tests/tier1_export.rs` holds the `dump` exporter to
  the same files at both tiers, to the byte.
  In the browser, hosting components inside a wasm app is unsolved in the
  ecosystem; a document using a Tier 1 plugin opens there with its
  features frozen until an ADR decides otherwise (`SEED.md` §9).
- **Tier 2** is one child process per plugin, started on first use, kept
  alive for the session, restarted after a crash. A crash or a timeout is
  a failed feature with a diagnostic, never a hung app. A feature builds
  through the host's kernel, called back over the wire while `evaluate`
  runs, and bodies cross as handles valid for that evaluation, never as
  bytes (ADR-0021). Out-of-process Python gets numpy, torch and CUDA for
  its own computation, and in remote mode the server's GPU.
  An exporter is the same, the other way round: the host calls
  `exchange.run`, and the plugin reads the document by `document.*`
  requests, answered from the export's snapshot while it runs. A
  `document.*` request without `document-read`, or one made during an
  evaluation, ends the call as `plugin.capability`; so does a `kernel.*`
  request during an export. The Python SDK's `@arrix.exporter` registers
  one (`python/examples/bom`, a CSV bill of materials), and `arrix.session`
  has `export_with` and `exported_files`.

The Tier 2 wire is JSON-RPC 2.0, one object per line, its methods and
shapes generated from the WIT world (ADR-0022): `kernel.extrude`,
`feature.evaluate`, a record as an object, a variant as a one-key object, a
`result` as `{"ok": …}` or `{"err": …}`, a resource as its handle, bulk
`f32` and `u32` runs as base64. `arrix serve --stdio` uses the same
framing.

A Tier 1 or Tier 2 plugin is loaded into a running session by a request,
not a command (ADR-0024): `session.load_plugin` takes a plugin directory
and loads it at the tier its manifest names, and `session.reload_plugin`
loads it again from where it was loaded (the process restarted, the
component re-read), so an edit to its source or a rebuilt `.wasm` is seen
with no rebuild of ArriX. A directory takes the place of a compiled-in
plugin of the same id (a Tier 1 `gears` for the Tier 0 one), here and in
`--plugin` everywhere. `arrix serve --plugin
<dir>` loads at start. Each makes a generation, and the features frozen
for lack of the plugin evaluate live. Unloading is not built. A plugin calls `kernel.*` while it answers `feature.evaluate`.

## The manifest

Every plugin, whatever its tier, has an `arrix-plugin.toml`:

```toml
[plugin]
id = "gears"                 # the namespace of everything it contributes; [a-z][a-z0-9-]*
version = "0.1.0"            # the plugin's own semver
api = "^0.4"                 # the arrix-plugin-api range it was built against
tier = 0                     # 0, 1 or 2
title = "Gears"
licence = "MIT OR Apache-2.0"

[tier2]                      # Tier 2 only
command = ["python", "-m", "arrix_fasteners"]
timeout-seconds = 30         # optional: the longest silence in a call

[tier1]                      # Tier 1 only (ADR-0035)
component = "gears.wasm"     # a relative .wasm path inside the plugin's directory
fuel = 1073741824            # optional: fuel per evaluation, up to 2^36 (default 2^30)

[capabilities]               # §Capabilities
kernel = true
document-read = true
document-write = true        # commands
files = "picked"             # "none" | "picked": only files the user picked in a host dialog
network = false

[contributes]                # informative; the plugin's exports are the authority
features = ["gears.spur"]
ribbon = ["gears"]

[test]                       # §The test kit; optional
documents = ["fixtures/gear-on-plane"]   # fixture documents, relative to the plugin
fail = ["fixtures/oversize"]             # fixtures expected to fail with a kernel category

[[test.exports]]                          # an exporter run on a fixture, twice
document = "fixtures/arm"
exporter = "robotics.urdf"
assembly = "arm"                          # or `part = "…"`, by name or id
options = { effort = "10" }               # as `arrix export --option`
```

`[test]` is read by `arrix test` and by nothing else. A fixture that is not
there, or a key the table does not know, is a diagnostic with the
manifest's file and line, as for any other table.

Contributed ids are `<plugin-id>.<name>`. `core.` is reserved for built-ins.
`arrix_plugin_host::Manifest::parse` refuses an unknown key, a `version`
that is not semver, an `api` that is not a range, a tier past 2, and a
`[tier2]` section on any tier but 2 (or its absence there), a `[tier1]`
section on any tier but 1 (or its absence there), a `component` that is
not a relative `.wasm` path inside the directory, a `fuel` outside 1 to
2^36 (each of those two naming its line), and a
`timeout-seconds` that is not a positive number (default 30). The timeout
bounds each wait for the plugin's next line, not a call: the kernel's time
while the plugin waits is the host's.
Capabilities default to off.
A plugin id is registered nowhere yet; a plugin index is a named, unordered
cycle (`docs/ROADMAP.md`).

## Contribution points

Each is an optional export of the world. The host calls a plugin; a
plugin never calls into the UI or the document except through the host
service it imports.

### 1. Features

A history entry with a form, named inputs and output slots, evaluated like
a built-in feature (`docs/DATA-MODEL.md` §Features): off the UI thread,
memoised by input hash, named from kernel provenance, and stored frozen in
the document.

- **`evaluate` is deterministic** (`SEED.md` §6.3): a pure function of its
  parameters, its resolved inputs and the plugin's version. No clock, no
  randomness, no reading files or the network. Memoisation, replay, frozen
  results and the kernel's fixture records all depend on it, and the test
  kit evaluates twice to check it. Its transcendental math is
  `arrix_plugin_api::math` (`libm`, compiled into the plugin), never
  `f64::sin` and its kin, which are the system's libm natively and Rust's
  on wasm32, so its result does not depend on its tier (ADR-0036);
  `plugins/gears/clippy.toml` enforces it with `disallowed-methods`.
- **Parameter defaults**: a feature type declares each parameter's default
  in its `ParamSpec`, and a user may override it in `settings.toml`
  (`[defaults."<type>"]`, ADR-0015). That only seeds a new feature's form;
  the committed value is the document's, and config never reaches
  `evaluate`. No plugin cooperation is needed.
- **Outputs** are bodies, datums, frames and sketches, placed in the slots
  the feature type declared.
- **Naming**: everything built through kernel operations is named from
  their provenance, so a plugin feature's faces are referenceable
  downstream like a built-in's. Topology a plugin builds directly (from
  profiles it constructs point by point, say) is rooted at a consumer role
  with a key the plugin supplies (Arris ask A1); the key must be stable
  across evaluations for the same logical entity. A name is opaque text
  here (grammar: `docs/DATA-MODEL.md` §Persistent naming); a feature's
  sweeps are numbered in the order it makes them (ADR-0011), so a feature
  that sweeps twice names its caps apart as long as it keeps that order.
- **Versioning**: a feature type has its own `type_version`. A document
  holding an older one is migrated by the plugin's `migrate`; a newer one
  than the plugin knows evaluates frozen.
- **Diagnostics** name what failed, in the plugin's terms, and may carry
  persistent references for the UI to highlight.

### 2. Document data

Plugin-owned records in the document, under `plugins/<plugin-id>/`,
versioned by the plugin's own schema and opaque to the core
(`docs/DATA-MODEL.md` §Frozen results). A record points at geometry by
`persistent-ref` (a robot link's bodies, an FEA load's faces) and at other
records by id. The core keeps a section byte for byte when its plugin is
absent.

### 3. Commands

Undoable edits, each a composition of core commands and edits to the
plugin's own data, applied atomically as one gesture. A plugin never
mutates the document any other way. The plugin expands the gesture on
the client into one `Group`, which is submitted; its undo entry is that
group's inverse, computed by the core, and a plugin-data edit is a
whole-record put or remove the core inverts without the plugin
(ADR-0004, `docs/DATA-MODEL.md` §Commands and undo). A plugin never
supplies an inverse.

### 4. Importers and exporters

- **Importer**: bytes of a file the user picked, in; commands, out. A
  feature-history dataset replayed into ArriX is an importer
  (`SEED.md` §6.6).
- **Exporter**: an evaluated snapshot, in (read through `document`); files,
  out (`exchange`, ADR-0037). The host writes them where the user chose:
  one file to the path, several into a directory there, so a URDF's meshes
  sit beside it, and an existing file is kept unless `--force`. URDF, SDF
  and MJCF are the robotics plugin's (`robotics.urdf` is built: options
  `effort`, `velocity` and the choice `mesh-prefix`, `relative` or
  `package`; SDF and MJCF are not). The core's STEP, STL and OBJ are
  registered as `core.step`, `core.stl` and `core.obj`, listed and
  reachable like any exporter, but implemented natively: STEP needs the
  B-rep, which the snapshot does not carry. They carry the product
  structure both ways (ADR-0032).

A plugin never receives a path, only bytes and a file name, so an
importer is the same on native, in the browser and on a remote server.

### 5. Analyses

Read-only jobs over an evaluated snapshot, with progress and cancellation
from the host service, returning a report (a form of read-only values) or
a viewport overlay (drawables, including a mesh with a colour field). They
never mutate the document; storing a result is a command the user issues
afterwards.

### 6. UI

**Declarative only**, rendered by the core (`docs/UI-RENDERING.md`
§Declarative plugin UI): ribbon groups and buttons bound to the plugin's
commands, a property form for each feature type and record kind, panels
built from form widgets, context-menu entries, and viewport drawables
(points, lines, frames, labels, meshes with a colour field, gizmos with
typed drag handles). A plugin never names egui. Declarative UI crosses a
process or wasm boundary, looks like the rest of the app, and renders
headless for tests. Letting a Tier 0 plugin draw its own egui is an escape
hatch decided only when a real plugin needs it, by ADR.

## Capabilities

The manifest declares what a plugin may do; the host grants that and no
more.

| Capability | Grants |
|---|---|
| `kernel` | the kernel host service |
| `document-read` | the `document` snapshot read, during an `exchange` call only (ADR-0037) |
| `document-write` | issuing commands |
| `files = "picked"` | bytes of files the user picked in a host dialog, for import; a save target for export |
| `network` | none by default; a Tier 1 plugin never gets sockets without the user's approval |

At Tier 1 the grants are enforced by the sandbox: the component's imports
are the only doors it has. At Tiers 0 and 2 the plugin is trusted code and
the capabilities are declarative: the host still refuses a call outside
them, so a plugin tested under its manifest behaves the same once
sandboxed. A Tier 2 host enforces `kernel`, `document-read` and
`document-write` at the wire, a refused call being a `plugin.capability`
diagnostic; `files` and `network` are declared and shown, not enforced,
since the process is the user's own Python (ADR-0021). A feature's `evaluate` gets `kernel` and its resolved inputs
only, whatever the manifest says, because determinism demands it.

## Versioning

- `arrix-plugin-api` is versioned by **semver apart from the app**
  (`SEED.md` §6.3), and the WIT package version is the crate's version,
  which the equality test checks. The first release was 0.1.0; 0.2.0
  added region references and region inputs (ADR-0006); 0.3.0 body
  inputs, slots that modify them, and the kernel's `fuse` and `cut`
  (ADR-0007, ADR-0008); 0.4.0 list inputs, the `edge`, `point`, `axis`
  and `tool` input kinds, the tool a feature keeps (ADR-0012, ADR-0013),
  and the kernel's `revolve`, `common`, `fillet`, `chamfer`, `copy` and
  `mirror`; 0.5.0 (breaking) the choices a type declares, `shown-when`,
  and the choices `evaluate` receives (ADR-0018); 0.6.0 (breaking) the
  `density` quantity kind (ADR-0031); 0.7.0 (breaking) the `document`
  import and the `exchange` export (ADR-0037).
  The `guest` feature (Tier 1 SDK, ADR-0035) adds API without touching
  the world, so the crate and the WIT package stayed at 0.6.0 then: the package
  version is the crate's, which a patch bump would ripple through every
  wire file, the Python SDK and each component's import names. The
  `math` module (ADR-0036) is additive in the same way.
  While it is `0.x`, a minor bump is breaking. Every change to it says its
  semver effect in the commit body (`.agents/rules/git.md`).
- The host supports one API range at a time and says which in `arrix
  --version`. A plugin whose `api` range does not overlap is not loaded;
  its features open frozen, with that reason.
- The plugin's own `version` is part of every feature's input hash, so
  upgrading a plugin re-evaluates its features.
- First-party plugins are in-tree (`plugins/`) until C3, where a breaking
  API change is one commit across all of them. At C3 they move to their
  own repositories under `arris-labs` (ADR-0003), which proves the out-of-tree path
  third parties use (`SEED.md` §9).

## Frozen fallback

A plugin feature evaluates frozen when its plugin is not installed, its
version or API range does not match, its tier cannot run here (Tier 2 in
the browser, Tier 1 in the browser until an ADR: a wasm build has no
`tier1` module, so a Tier 1 plugin is not in its registry and its features
are frozen as any plugin's absent), or the user disabled it.
The reason is shown on the feature and in `arrix eval`'s output. The
states, staleness and re-freezing are in `docs/DATA-MODEL.md` §Frozen
results.

## The test kit

A plugin is tested headless, with no window and no human, the same way
ArriX tests itself. `arrix test <plugin-dir> [--plugin <dir>]… [--schema]`
runs the checks below on any plugin directory, whatever its tier, and
`plugins/gears` (built in, or as a Tier 1 component) and `python/examples/standoff` pass it. At Tier 1 a fresh store per evaluation is the determinism check's fresh process. The plugin's
manifest names its fixture documents in `[test]` (§The manifest), kept
inside the plugin's directory so a plugin outside the repository is as
self-contained as these (`plugins/gears/fixtures/gear-on-plane`,
`python/examples/standoff/fixtures/standoff`). `--plugin` adds the
directories of plugins the fixtures depend on.

- **Output**: one JSON line per check, `{format, plugin, check, doc,
  status, detail}`, `status` being `pass` or `fail` and `detail` saying
  what was compared or what differed. The report is versioned and
  `--schema` prints its schema (ADR-0026). Exit status 0 when every check
  passed, 1 when one failed, 2 when the plugin cannot be tested at all (an
  invalid manifest, a fixture that is not there).
- **Determinism**: each fixture evaluated twice, each on a fresh kernel and
  a fresh Tier 2 process, saves to identical bytes.
- **Round trip**: save, open, save gives identical bytes and an identical
  eval line.
- **Frozen**: the document opened without the plugin has each of its
  features frozen with a reason, each one with a stored result keeping its
  bodies, and downstream features evaluate.
- **Migration**: every `type_version` the plugin's specs declare has a
  fixture holding it, and that fixture evaluates. No feature type has a
  second version yet, so this covers fixtures only; `migrate` is not in the
  WIT.
- **Spec**: every feature spec validates (ADR-0018). A valid spec is a
  renderable form, which is why the kit does not render forms: `arrix`
  never links wgpu (ADR-0025).
- **Exports**: each `[[test.exports]]` entry runs its exporter twice, each
  on a fresh registry (a fresh Tier 1 store or Tier 2 process), to identical
  files (ADR-0037 §9); the detail names the files, their sizes and hashes.
  **Spec** also holds the manifest's `contributes.exporters` to the
  exporters the plugin registers.
- **Kernel failures**: each `fail` fixture fails with a `kernel.*`
  category and leaves a call record that replays (`SEED.md` §6.6).

The kernel-backed checks of a Tier 0 plugin's own behaviour (names, volume
against the outline, the names a teeth edit keeps) stay in
`crates/arrix-cli/tests/gears.rs` and `gear_naming.rs`, since a plugin
depends on the API alone. The Python SDK has `python -m unittest`, run by
`crates/arrix-plugin-host/tests/python_sdk.rs`, and the standoff's checks
are in `crates/arrix-cli/tests/boundary_m1.rs`. `arrix-plugin-test`, the
Rust kit for Tier 0 authors, comes with C3 beside the Tier 1 SDK.

An agent reaches all of it with the installed binaries and the SDK:
`arrix.session` drives `arrix serve --stdio` and `arrix.eval` runs the
eval line (ADR-0026). `cargo test -p arrix-cli --test boundary_m3` is that
scenario.

## Open questions

None open. The Tier 2 wire encoding was settled by ADR-0022.
