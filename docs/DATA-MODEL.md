# Data model

What a document is, how it changes, how it evaluates, how its geometry is
named and how it is written to disk. The charter is `SEED.md` §6.4–§6.5;
this document is the design those sections commit to. Nothing here is built
yet: each section becomes true as C1 lands, and the commit that builds it
keeps it true.

Types live in `arrix-doc` unless named otherwise. Persistent-reference types
live in `arrix-core`, so that `arrix-plugin-api` and `arrix-ui` can carry
them without naming the document.

## The document

A document is a **recipe**: everything needed to regenerate its geometry,
and nothing that can be regenerated.

```rust
pub struct Document {
    schema: SchemaVersion,              // the .arrx schema, §File format
    generation: Generation,             // bumped by every applied command; runtime only
    params: Params,                     // named, typed parameters and expressions
    parts: BTreeMap<PartId, Part>,      // a part is a feature history
    plugin_data: BTreeMap<PluginId, PluginSection>,
    meta: Meta,                         // title, units shown, plugins used; no timestamps
}

pub struct Part {
    id: PartId,
    name: String,
    history: Vec<FeatureId>,            // the order the user sees and edits
    features: BTreeMap<FeatureId, FeatureRecord>,
    rollback: Option<usize>,            // features at and after this index are not evaluated
}
```

Assemblies (components, instances, joints) are C2's and extend this type
with an `assemblies` map; the DAG already admits cross-part references, so
they add node kinds, not a new mechanism.

**What is not in the document** (`SEED.md` §6.1 rule 4): selection, hover,
the active sketch, the edit mode, the camera, panel layout, and every
evaluated result. Those are session state, owned by the client. Evaluated
geometry is cache, except a frozen feature's last result (§Frozen results),
which is recipe because nothing else can regenerate it.

### Identifiers

Every addressable record (part, feature, parameter, sketch entity, plugin
record) has an id minted **by the author of the command that creates it**,
carried in that command: 64 bits, written as 13 characters of Crockford
base32. Minting on the author's side keeps a command self-contained: its
inverse names the same id, a later command by the same author can reference
it before the authority has acknowledged it, and replaying a command log
reproduces the document byte for byte. An id is never reused within a
document; a collision (vanishingly rare with random ids) is a rejected
command, not a rename.

The id type is `arrix_core::Id` (a `u64`), wrapped by one typed id per
kind of record: `PartId`, `FeatureId`, `ParamId`, `SketchEntityId`,
`RecordId`. Its text is 13 characters of Crockford's alphabet, most
significant first, so the first is `0`–`F`; written upper-case, read in
either case with Crockford's aliases (`I` and `L` as 1, `O` as 0); a JSON
string both ways. An `IdMinter` (SplitMix64) mints them from a seed its
caller passes: `arrix-core` reads no entropy source, so the same seed
gives the same ids on every target, a `const` assertion pinning the first
ids of seed 1 in every build, the wasm one included. Where a client's
seed comes from is decided with sessions (M1).

Ids are identity, not order. `history` holds the order; maps are
`BTreeMap`s keyed by id so the serialised form is sorted.

### Parameters and expressions

A parameter is a name, a quantity kind (length, angle, count, ratio,
mass, …) and an expression. An expression is a small language: numbers
with unit suffixes (`12 mm`, `30 deg`), `+ - * / ^`, parentheses, the usual
functions (`sin`, `sqrt`, `min`, `max`, `abs`, `round`) and parameter
names. It is parsed once into an AST stored as its source text; the text is
the recipe, so a diff shows what the user typed.

- **Dimensional analysis at parse time.** An expression whose unit does not
  match its slot (a length where an angle is wanted) is a diagnostic on
  that parameter, not a silent conversion.
- **SI inside.** Values evaluate to metres, radians, kilograms. The UI shows
  mm and deg by default; display units are a document preference in `meta`,
  never a scale applied to stored values. The unit table
  (`arrix_core::units`): `mm`, `cm`, `m`, `in` (25.4 mm exactly), `deg`,
  `rad`, `g`, `kg`, each an exact ratio to SI; count and ratio are
  dimensionless. A `Quantity` is its kind and its SI value.
- Every numeric field of a feature (built-in or plugin) and every sketch
  dimension holds an expression, not a number. A plain number is the
  expression `12 mm`.
- A parameter referenced by an expression is a DAG edge (§The dependency
  DAG). A cycle between parameters is refused when the command that would
  create it is applied.

## Features

A feature is one history entry. Built-in and plugin features have **one
record shape**; that is the point of C1 (`SEED.md` §7).

```rust
pub struct FeatureRecord {
    id: FeatureId,
    type_id: FeatureTypeId,        // "core.extrude", "gears.spur": namespaced, stable
    type_version: u32,             // the feature type's own schema version
    name: String,                  // what the tree shows; unique within the part
    params: ParamValues,           // the Form's fields, each an expression or a plain value
    inputs: BTreeMap<String, Ref>, // named inputs: a plane, a profile, faces, a body
    suppressed: bool,
    frozen: Option<FrozenResult>,  // §Frozen results; always present for a plugin feature
}
```

The feature *type* (its parameters' form, its inputs, its `evaluate`) comes
from a registry. Built-in types register under the reserved `core.`
namespace through the same `FeatureType` shape a plugin's `Feature` is
adapted onto (`docs/PLUGINS.md` §Features), so the evaluator has one code
path for both. A `type_id` the registry does not know is not an error: the
feature evaluates as frozen.

A feature's outputs are **slots**: named bodies, datums (plane, axis,
point, frame) and sketches. `core.extrude` in new-body mode has one body
slot, `body`; in cut or join mode it has none of its own and modifies the
target's slot (§Bodies across features).

### The core feature types

| Type | Inputs | Kernel operation | Cycle |
|---|---|---|---|
| `core.sketch` | a plane: a datum, a planar face, or a frame | none in the kernel; `arrix-sketch` solves it; its closed regions are profiles | C1 |
| `core.datum-plane` | offset from a plane or face, through three points, at an angle about an edge | none | C1 |
| `core.extrude` | a profile (sketch regions), a direction, a mode: new body, join, cut | `ops::extrude`, then `fuse` or `cut` | C1 |
| `core.revolve` | a profile, an axis, a mode | `ops::revolve`, then `fuse` or `cut` | C1 |
| `core.fillet`, `core.chamfer` | edges of one body | `ops::fillet`, `ops::chamfer` | C1 |
| `core.hole` | points on a planar face (sketch points), diameter, depth or through, counterbore / countersink | a revolved tool profile, then `cut` | C1 |
| `core.pattern-linear`, `core.pattern-circular` | features or a body, count, spacing | `ops::transform` of the tool or body, then `fuse`/`cut` | C1 |
| `core.boolean` | a target body, tool bodies, an operation | `fuse`, `common`, `cut` | C1 |
| `core.mirror` | features or a body, a plane | `ops::mirror` (Arris ask A11) | C1 once A11 is released, else the first cycle after |

Sweep, loft, shell, draft and split wait for their Arris cycles
(`SEED.md` §6.2). A type that is not in this table is not planned around.

### Bodies across features

A part's bodies are named slots that features create, modify and consume.
A body is referenced as *slot `s` as of feature `f`*: after an extrude in
cut mode, the body it cut is a new version of the same slot. A reference
to a body means the version current at the referencing feature's position
in the history, so inserting a feature earlier moves every later reference
with it. Deleting a body (a boolean's tools) ends its slot.

## References

A reference is how one record points at another. It is plain data, lives
in `arrix-core`, and resolves or fails; it never guesses.

```rust
pub enum Ref {
    Param(ParamId),
    Feature(FeatureId),
    Slot { feature: FeatureId, slot: SlotName },          // a body, datum or sketch output
    Topo(PersistentName),                                  // a face, edge or vertex
    Sketch { feature: FeatureId, entity: SketchEntityId }, // a sketch point or curve
    Plugin { plugin: PluginId, record: RecordId },         // a plugin document record
}
```

Every `Ref` is a DAG edge. `Topo` references are the ones naming solves;
the rest are by id.

## The dependency DAG

The DAG exists from day 0 (`SEED.md` §6.5). Its nodes are parameters,
features, parts and plugin records; its edges are every `Ref` in a record
and every parameter name in an expression. It is derived from the
document, never stored, and rebuilt incrementally by the command that
changes an edge.

- **Within a part**, a feature references only features before it in
  `history`. Reordering is a command that is refused when it would break
  that.
- **Across parts**, a feature may reference another part's feature output
  (a sketch projected from another part's face, a body used as a tool).
  Such an edge makes the whole referenced prefix of the other part an
  input.
- **A cycle is refused** at command application, with the cycle's nodes in
  the diagnostic. The document never holds a cyclic graph.
- **Dirty propagation**: a command marks the nodes it touched; the
  evaluator re-evaluates their descendants in topological order, and a
  node whose inputs hash the same as last time is a cache hit (§Evaluation).

## Evaluation

The evaluator turns a snapshot of the document at generation `g` into
outputs, off the UI thread (`docs/CONCURRENCY-WASM.md` §The evaluator).

1. Resolve each feature's parameters (expressions → SI values) and inputs
   (`Ref` → resolved geometry, or a diagnostic).
2. Compute the feature's **input hash**: BLAKE3 over the canonical
   serialisation of `type_id`, `type_version`, the plugin's version, the
   resolved parameter values and the hashes of the resolved inputs. The
   Arris version is part of the hash of every kernel result.
3. On a cache hit, reuse the output. Otherwise call the feature type's
   `evaluate` with a kernel context (`arrix-kernel`), which records every
   kernel call it makes (`docs/ARCHITECTURE.md` §The kernel choke point).
4. Name the outputs from the kernel's provenance (§Persistent naming).
5. Publish an `EvalEvent` per feature: done with its outputs, failed with
   a diagnostic, or frozen with its reason.

**Fail soft.** A failed feature keeps its parameters, shows why, and
produces no outputs; a downstream feature that needs one of them fails
with *input unavailable* naming it; features that do not depend on it
evaluate as normal. No retry with a nudged tolerance, ever
(`SEED.md` §8.2).

**Determinism.** Evaluating the same snapshot twice, on any machine,
native or wasm, gives byte-identical outputs and names. Nothing in
evaluation reads the clock, a random source or a `HashMap`'s iteration
order.

## Persistent naming

A face, edge or vertex is named by **where it came from**, computed from
Arris's `Provenance`, never by where it is (`SEED.md` §6.5). The kernel
guarantees the accounting (every output entity is kept or has an origin)
and the split order (the `k` in `Split(k)` means the same piece after an
edit that keeps which entities bound which piece); ArriX supplies the
words. Arris ships no name grammar by design.

```rust
pub struct PersistentName {
    root: NameRoot,           // where the chain starts
    chain: Vec<NameStep>,     // what happened to it since, feature by feature
    kind: TopoKind,           // Face | Edge | Vertex
}

pub enum NameRoot {
    Sweep { feature: FeatureId, part: SweepPartName },   // cap, side of sketch curve c, …
    Primitive { feature: FeatureId, role: PrimitiveRole },
    Plugin { feature: FeatureId, key: u64 },             // Arris ask A1, consumer roles
    Imported { feature: FeatureId, entity: u64 },        // a file's own entity (C2)
    Frozen { feature: FeatureId, index: u32 },           // a frozen result's stored name table
}

pub enum NameStep {
    Modified { feature: FeatureId, split: u32 },         // a piece of it; split 0 when whole
    Generated { feature: FeatureId, from: Vec<PersistentName> }, // e.g. an edge from two faces
}
```

- **Roots** come from what made an entity from nothing. A sweep's side
  face is rooted at the sketch **entity id** of the curve that swept it,
  not at its loop index: `arrix-kernel` translates sketch ids to Arris's
  `(loop_index, segment)` on the way in and back on the way out, so
  re-ordering or re-drawing a sketch keeps names on the curves that
  survive.
- **Generated entities** name their origins: the rim of a hole is the edge
  generated from the pair *(hole wall, top face)*. Edges and vertices
  derive from their faces where Arris records them that way.
- **Kept entities** keep their names across features; a kept id is not a
  step.
- **Plugin features get naming for free** when they build through kernel
  operations. Topology a plugin builds directly is rooted at a consumer
  role with a key the plugin chooses (ask A1).

**Resolution.** A `Ref::Topo` resolves against the current evaluation to
exactly one entity or to a diagnostic. A name that matches nothing, or
more than one entity, is a **lost reference**: the feature fails with a
diagnostic that lists candidates, ranked by the longest shared chain
prefix, and the user (or an agent) re-picks by command. It is **never
rebound silently** (`SEED.md` §8.2), and never matched by centroid,
normal or area.

## Frozen results

A document opens without the plugins that made it (`SEED.md` §6.4).

Every plugin feature stores its last successful result in the document:

```rust
pub struct FrozenResult {
    input_hash: Hash,                   // §Evaluation step 2, at the time it was stored
    plugin_version: semver::Version,
    slots: BTreeMap<SlotName, BlobRef>, // Arris body bytes (ask A2), one blob per body
    names: BlobRef,                     // the persistent-name table of every stored entity
}
```

It is written whenever the feature evaluates successfully with its plugin
present, so the file always carries the latest result.

A plugin feature evaluates in one of four states:

| State | When | Shown as | Parameters |
|---|---|---|---|
| **Live** | the plugin is present, its version is in the feature's range and its tier can run here | a normal feature | editable |
| **Frozen** | the plugin is missing, the wrong version, or its tier cannot run here (Tier 2 in a browser) | the stored result, with a badge naming the reason | read-only |
| **Frozen, stale** | frozen, and its current input hash differs from the stored one (an upstream edit changed its inputs) | the stored result, with a stale badge | read-only |
| **Failed** | live, and `evaluate` returned an error | no output, the diagnostic | editable |

The settled default for staleness (`SEED.md` §10.5): an upstream edit
that changes a frozen feature's inputs **keeps the old result**, marks it
stale, and it is re-frozen from a live evaluation the next time its plugin
is present. Downstream features evaluate against the stored result in both
frozen states; the stale badge propagates to them as a warning, not a
failure.

A frozen result's names come from its stored name table, so references
into it resolve exactly as they did when it was stored.

**Plugin document data without its plugin** is kept byte for byte and
written back unchanged. The core never parses a section it has no plugin
for. A core command that deletes geometry a plugin record points at leaves
the record dangling; the plugin reports it as a diagnostic when it is
next present.

## Commands and undo

A command is the only way the document changes (`SEED.md` §6.1). It is
plain, serialisable, versioned data: no closures, handles or pointers into
the document.

```rust
pub struct CommandEnvelope {
    author: AuthorId,          // the local user in local mode
    base: Generation,          // the generation the author saw
    command: Command,
}

pub enum Command {
    SetParam { param: ParamId, expr: String },
    AddFeature { part: PartId, at: usize, record: FeatureRecord },
    EditFeature { feature: FeatureId, params: ParamValues, inputs: InputEdits },
    DeleteFeature { feature: FeatureId },
    ReorderFeature { feature: FeatureId, to: usize },
    SketchEdit { feature: FeatureId, edit: SketchEdit },   // entities, constraints, dimensions
    SetRollback { part: PartId, at: Option<usize> },
    PluginData { plugin: PluginId, edit: RecordEdit },     // opaque to the core
    Group { label: String, commands: Vec<Command> },       // one undo entry
    // …
}
```

Applying a command is a pure function of the document and the command:
it returns the new document, its **inverse** command, and the set of nodes
it touched. It either applies whole or not at all.

- **Generations.** Every record carries the generation that last changed
  it. A command applies if nothing it reads or writes changed after its
  `base`; otherwise it is rejected as **stale** and the author rebuilds it
  against the new state. There is one writer, the document, so there is no
  merge and no CRDT. Locally the base is always current.
- **Undo is per author, by inverse commands.** Each author has an undo and
  a redo stack of inverses. Undo applies the top inverse as an ordinary
  command, subject to the same staleness rule; an undo that would clobber
  another author's later change is stale and refused. Undo never restores
  a snapshot.
- **One gesture is one command** (`SEED.md` §8.2). A multi-step gesture
  (a sketch drag that adds a point and a coincidence) is one `Group`. A
  command whose effect equals the current state is not applied and leaves
  no undo entry.
- **Plugin commands** are composed from core commands and the plugin's own
  `PluginData` edits (`docs/PLUGINS.md` §Commands).

⚠ OPEN: undo granularity for plugin commands (C1). A plugin command
expands into core commands and plugin-data edits; whether its undo entry is
the expansion as one `Group`, or the plugin supplies its own inverse over
its data, decides whether a plugin can have undo that is not the literal
reverse of its steps. Within `SEED.md` §6.1's per-author inverse rule
either way. Agent proposes, by the C1 step that adds plugin commands; the
ADR records it.

## Sketches

A sketch is the record of a `core.sketch` feature; its model and solver
are `arrix-sketch` (`docs/UI-RENDERING.md` §Sketch mode for the
interaction).

- **Entities**: points, lines, arcs and circles, each with an id and a
  construction flag. Ellipses, elliptic arcs and B-splines are ported
  behind a feature gate that stays off until a scenario reaches them
  (`SEED.md` §8.2 rule 7). Projected geometry (an
  edge or vertex of a body projected onto the plane) is an entity with a
  `Ref::Topo` source, re-projected on evaluation and failing like any lost
  reference.
- **Constraints and dimensions** reference entities by id. A dimension is
  a constraint with a value, driving or reference, and its value is an
  expression, so it can name a parameter. The kinds a document can hold
  are the kinds the UI can create; the solver may know more.
- **The last solved positions are recipe.** They are the solver's starting
  point, and with a nonlinear solver the starting point selects which of
  several solutions it lands on. Stored positions make re-solving
  deterministic and keep a sketch on its branch after an edit.
- **Profiles** are the faces of the planar arrangement of the
  non-construction curves. A feature references a region by a key: the
  ids of the entities that bound it plus an interior sample point. A key
  that resolves to no region, or to two, is a lost reference, never the
  nearest region. Arris ask A5 (`region2` as public API) would make the
  arrangement the kernel's own, so what the sketch shades is what extrude
  accepts by construction; until it lands the arrangement is
  `arrix-sketch`'s.
- A sketch placed on a face follows the face's frame on re-evaluation
  (`ops::query::face_frame`); a lost face is a lost reference.

## File format

A document is saved as `.arrx`: a zip whose unzipped form is itself a
valid document directory (`SEED.md` §6.5). Both forms open.

```
document.json          schema version, ArriX version that wrote it, meta, plugins used
params.json            the parameters
parts/<part-id>.json   one part: history order, feature records, sketches
plugins/<plugin-id>/   one plugin's section: its own schema version and its records
blobs/<blake3>.<ext>   frozen results (.arrisbody), name tables, imported files, thumbnails
```

- **Deterministic JSON.** Keys sorted, two-space indent, `\n` line ends, a
  trailing newline, floats in shortest round-trip form. Saving the same
  document twice gives identical bytes; so does saving, loading and saving
  again. A test holds both.
- **Deterministic zip.** Entries in sorted path order, a fixed timestamp,
  JSON deflated and blobs stored. The same document gives the same `.arrx`.
- **Blobs are content-addressed**: the name is the BLAKE3 of the bytes, so
  identical results are stored once and a changed result is a new file.
  Blobs no record points at are dropped on save.
- **Versioning.** `document.json` carries `schema` (an integer, 1 at C1).
  Opening an older schema runs forward migrations in order, each a pure
  function over the JSON tree with its own fixture document. A newer
  schema than the build knows is refused with a message naming both. A
  feature record's `type_version` is migrated by its feature type (a
  plugin's own migration, or kept frozen if the plugin is absent).
- **Plugin sections** carry their plugin's schema version and are opaque
  to the core (§Frozen results).
- **Units** in the file are SI; `meta` records the display units.
- **What is not in the file:** generations and the per-record
  generation stamps (they are the authority's runtime state, so undo can
  return a document to earlier bytes), session state, evaluated geometry
  other than frozen results, absolute paths, the author's machine or name
  (authorship metadata is a later decision, with its own ADR), timestamps
  in the recipe.

## Open questions

- ⚠ OPEN: undo granularity for plugin commands (§Commands and undo). C1;
  agent proposes; ADR.
