# Plugins

The plugin model is ArriX's reason to exist (`SEED.md` §3, §6.3): one
interface, versioned apart from the app, hosted three ways, and enough of it
that first-party domains need nothing else. This document is the design of
`arrix-plugin-api` and `arrix-plugin-host`. Built: the world and its Rust
traits at 0.1.0 (§One interface), manifests (§The manifest) and Tier 0
hosting (§Three tiers). Each other section becomes true as its
cycle lands (Tier 0 in C1, Tiers 1 and 2 and the test kit in C3), and the
commit that builds it keeps it true.

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
  a thin runtime speaking the Tier 2 wire protocol (C3).

The world as it stands at 0.1.0 (the file is the authority; comments and
field lists are elided here):

```wit
package arrix:plugin@0.1.0;

interface types {                                    // SI; no Arris type anywhere
  type id = u64;                                     // arrix-core's typed ids
  record vec2, vec3, frame { origin, x-axis, z-axis }
  enum quantity-kind { length, angle, count, ratio, mass }
  record quantity { kind, si: f64 }
  record persistent-ref { encoded: string }          // a PersistentName's text form, opaque here
  variant reference { param, feature, slot, topo, sketch, plugin }   // arrix-core's Ref
  record diagnostic { severity, code, message, refs: list<reference>, candidates: list<reference> }
  record profile { plane: frame, outer: profile-loop, holes: list<profile-loop> }
  // profile-loop: a keyed circle, or a path of keyed line and arc segments
  record mass-properties { volume, area, centroid }
}

interface kernel {                                   // a host service; imported by the plugin
  resource body;                                     // an opaque handle, valid within one evaluation
  extrude: func(p: profile, distance: f64) -> result<body, diagnostic>;
  names: func(b: borrow<body>) -> result<list<persistent-ref>, diagnostic>;
  face-frame: func(b: borrow<body>, face: persistent-ref) -> result<frame, diagnostic>;
  measure: func(b: borrow<body>) -> result<mass-properties, diagnostic>;
}

interface feature {                                  // exported by a plugin that adds features
  describe: func() -> list<feature-type-spec>;      // id, version, title, params, inputs, output slots
  evaluate: func(type-id: string, params: list<param-value>, inputs: list<resolved-input>)
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
  a world plane) is a `frame`. Output slots hold a `body` or a `plane`.
- **Profiles are plain records**, keyed curve by curve like
  `arrix_core::Profile`, since a key is what roots a side face's name.
- **What grows it, by minor version** (breaking while 0.x): `migrate`
  with a feature type's second `type_version`; `revolve`, `boolean`,
  `fillet`, `transform`, primitives and body bytes (ask A2) as the kernel
  service gains them; `import host` (log, progress, cancellation check,
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
profiles, meshes, persistent references) and, across a process boundary,
Arris's body bytes (ask A2). Kernel operations are a host service with
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
| 2 | a separate process speaking the interface over stdio | a child process and a wire codec | native only | full: it is the user's own Python | C3 |

- **Tier 0 goes through the same trait objects** the Tier 1 bindings
  produce. A first-party plugin that works at Tier 0 is therefore proof the
  public API is enough; one that needs more changes the API.
- **Tier 0 registration is an explicit list** in `arrix-app` and
  `arrix-cli` (`crates/arrix-cli/src/plugins.rs`: `register_tier0(&mut
  registry, arrix_gears::MANIFEST, Arc::new(arrix_gears::Gears))`), not
  linker tricks, which do not work on wasm. A plugin crate exports its
  manifest's text (`MANIFEST`, `include_str!` of its
  `arrix-plugin.toml`) and its `Feature`. A build without a plugin is a
  build that leaves it off the list, and the CLI can also disable a
  compiled-in plugin for a run (`--without-plugin <id>`), which is how the
  frozen path is tested.
- **Tier 0 as built** (`arrix_plugin_host::register_tier0`): the
  manifest must say tier 0 and an `api` range holding this host's
  `API_VERSION`; every type the plugin describes must be
  `<plugin-id>.<name>`; all its types register or none do. Each is
  adapted onto `arrix_doc::FeatureType` through the plugin API's traits
  only, with the manifest's `version` as its plugin version (in every
  input hash). A plugin type takes no choices. A panic in `evaluate` is
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
  a failed feature with a diagnostic, never a hung app. Bodies cross as
  body bytes plus persistent names. Out-of-process Python gets numpy,
  torch and CUDA, and in remote mode the server's GPU.

⚠ OPEN: the Tier 2 wire encoding, JSON-RPC or a binary encoding of the
WIT types (C3). Agent proposes; ADR. The constraint either way: the
encoding is derived from the WIT world, not written by hand beside it.

## The manifest

Every plugin, whatever its tier, has an `arrix-plugin.toml`:

```toml
[plugin]
id = "gears"                 # the namespace of everything it contributes; [a-z][a-z0-9-]*
version = "0.1.0"            # the plugin's own semver
api = "^0.1"                 # the arrix-plugin-api range it was built against
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
- **Outputs** are bodies, datums, frames and sketches, placed in the slots
  the feature type declared.
- **Naming**: everything built through kernel operations is named from
  their provenance, so a plugin feature's faces are referenceable
  downstream like a built-in's. Topology a plugin builds directly (from
  profiles it constructs point by point, say) is rooted at a consumer role
  with a key the plugin supplies (Arris ask A1); the key must be stable
  across evaluations for the same logical entity.
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
sandboxed. A feature's `evaluate` gets `kernel` and its resolved inputs
only, whatever the manifest says, because determinism demands it.

## Versioning

- `arrix-plugin-api` is versioned by **semver apart from the app**
  (`SEED.md` §6.3), and the WIT package version is the crate's version,
  which the equality test checks. The first release is 0.1.0.
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
ArriX tests itself. C1 has the pieces in-tree for the first plugin; C3
packages them for third parties (`arrix-plugin-test` for Rust, `arrix
test` for any tier).

- **Determinism**: every feature evaluated twice from fresh state gives
  byte-identical bodies and names.
- **Round trip**: a document using the plugin saves and loads to identical
  bytes and identical evaluation.
- **Frozen**: the document opened without the plugin shows every body,
  its features frozen with a reason, and downstream features evaluate.
- **Migration**: every `type_version` the plugin ever shipped has a fixture
  document that migrates and evaluates.
- **Declarative UI**: each form and panel renders headless and appears in
  `debug_state()` (`docs/UI-RENDERING.md` §Visual debugging).
- **Kernel failures**: a feature's kernel failure leaves a replayable call
  record, like a built-in's.

## Open questions

- ⚠ OPEN: the Tier 2 wire encoding (§Three tiers). C3; agent proposes; ADR.
