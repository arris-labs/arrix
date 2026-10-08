# Plugins

The plugin model is ArriX's reason to exist (`SEED.md` §3, §6.3): one
interface, versioned apart from the app, hosted three ways, and enough of it
that first-party domains need nothing else. This document is the design of
`arrix-plugin-api` and `arrix-plugin-host`. Built, as of C1: the world and
its Rust traits at 0.5.0 (§One interface), manifests (§The manifest), Tier 0
hosting (§Three tiers), the in-tree test kit's determinism and round-trip
checks, and the first plugin, `plugins/gears` with `gears.spur` (its crate
docs state its geometry and tolerance). Tier 2 and the packaged test kit
come with the boundary-opened cycle, Tier 1 with C3 (ADR-0016).

## One interface, defined once

The interface is a WIT world (the WebAssembly component model's IDL),
`wit/arrix-plugin.wit` in `arrix-plugin-api`, package `arrix:plugin` at the
API's version. From it come:

- the **Rust traits and plain types** of `arrix-plugin-api`, which Tier 0
  plugins implement directly and Tier 1 plugins implement through
  `wit-bindgen`'s guest bindings;
- the **host side** in `arrix-plugin-host`: an adapter per tier that
  presents a plugin, however hosted, as the same trait objects;
- the **Python SDK** (`python/arrix`): stubs generated from the world, and
  a thin runtime speaking the Tier 2 wire protocol (the boundary-opened
  cycle).

The world as it stands at 0.5.0 (the file is the authority; comments and
field lists are elided here):

```wit
package arrix:plugin@0.5.0;

interface types {                                    // SI; no Arris type anywhere
  type id = u64;                                     // arrix-core's typed ids
  record vec2, vec3, frame { origin, x-axis, z-axis }
  enum quantity-kind { length, angle, count, ratio, mass }
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
  export feature;
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

**The Rust side mirrors it**: `Kernel` and `Feature` traits and plain
types, with `arrix-core`'s own types (`Frame`, `Profile`,
`PersistentName`, `Ref`, `Diagnostic`, `Quantity`) re-exported where the
world's type is theirs, so a plugin names them through
`arrix-plugin-api` and the host passes them unconverted. `Feature::
evaluate` takes the kernel as `&mut dyn Kernel`, where the world imports
it. `crates/arrix-plugin-api/tests/wit_equality.rs` holds the two equal
three ways: every world type converts to its Rust type and back with
exhaustive destructuring on both sides, and round trips; `feature`'s
generated `Guest` is implemented over a Rust `Feature` and the reverse,
and the Rust `Kernel` over the generated imports, so a function added on
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
- **Tier 1** runs each component in its own `wasmtime` store, with fuel as
  the deterministic step budget and an epoch interrupt for cancellation.
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

The Tier 2 wire is JSON-RPC 2.0, one object per line, its methods and
shapes generated from the WIT world (ADR-0022): `kernel.extrude`,
`feature.evaluate`, a record as an object, a variant as a one-key object, a
`result` as `{"ok": …}` or `{"err": …}`, a resource as its handle, bulk
`f32` and `u32` runs as base64. `arrix serve --stdio` uses the same
framing. A plugin calls `kernel.*` while it answers `feature.evaluate`.

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

[capabilities]               # §Capabilities
kernel = true
document-read = true
document-write = true        # commands
files = "picked"             # "none" | "picked": only files the user picked in a host dialog
network = false

[contributes]                # informative; the plugin's exports are the authority
features = ["gears.spur"]
ribbon = ["gears"]
```

Contributed ids are `<plugin-id>.<name>`. `core.` is reserved for built-ins.
`arrix_plugin_host::Manifest::parse` refuses an unknown key, a `version`
that is not semver, an `api` that is not a range, a tier past 2, and a
`[tier2]` section on any tier but 2 (or its absence there).
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
  kit evaluates twice to check it.
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
- **Exporter**: an evaluated snapshot, in; bytes, out. The host writes them
  where the user chose. URDF, SDF and MJCF are the robotics plugin's.

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
| `document-read` | snapshot reads beyond the feature's own resolved inputs |
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
  and the choices `evaluate` receives (ADR-0018).
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
the browser, Tier 1 in the browser until an ADR), or the user disabled it.
The reason is shown on the feature and in `arrix eval`'s output. The
states, staleness and re-freezing are in `docs/DATA-MODEL.md` §Frozen
results.

## The test kit

A plugin is tested headless, with no window and no human, the same way
ArriX tests itself. C1 has the pieces in-tree for the first plugin; the
boundary-opened cycle packages them for third parties (`arrix-plugin-test` for Rust, `arrix
test` for any tier).

- **Determinism**: every feature evaluated twice from fresh state gives
  byte-identical bodies and names. Built for `gears`, with the kernel-backed
  checks a plugin's own tests cannot make (it depends on the API alone),
  in `crates/arrix-cli/tests/gears.rs` beside the registration list:
  names, volume and call records equal across fresh kernels, the volume
  against the outline's own area, and the names a teeth edit keeps;
  `crates/arrix-cli/tests/gear_naming.rs` references the gear's faces
  downstream by name through edits, and a lost one's candidates.
- **Round trip**: a document using the plugin saves and loads to identical
  bytes and identical evaluation. Built for `gears` by
  `crates/arrix-cli/tests/gear_on_plane.rs` and the
  `tests/docs/gear-on-plane` golden.
- **Frozen**: the document opened without the plugin shows every body,
  its features frozen with a reason, and downstream features evaluate.
- **Migration**: every `type_version` the plugin ever shipped has a fixture
  document that migrates and evaluates.
- **Declarative UI**: each form and panel renders headless and appears in
  `debug_state()` (`docs/UI-RENDERING.md` §Visual debugging).
- **Kernel failures**: a feature's kernel failure leaves a replayable call
  record, like a built-in's.

## Open questions

None open. The Tier 2 wire encoding was settled by ADR-0022.
