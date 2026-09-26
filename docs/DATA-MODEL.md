# Data model

What a document is, how it changes, how it evaluates, how its geometry is
named and how it is written to disk. The charter is `SEED.md` §6.4–§6.5;
this document is the design those sections commit to. Built: the id types
(§Identifiers), `Ref` and `SlotName` (§References), the persistent-name
types and their text form (§Persistent naming), the document's types,
the feature-type registry and the derived DAG (§The dependency DAG),
commands, the authority and per-author undo (§Commands and undo, less
plugin-data edits), the sketch record and its `SketchEdit` (§Sketches),
saving and opening the directory form (§File format), names from an
extrude's provenance in `arrix-kernel` (§Persistent naming), expressions
(`arrix_doc::expr`, §Parameters and expressions), the evaluator with its cache (§Evaluation),
`core.datum-plane`, and a plugin feature (`gears.spur`) on the built-ins'
one path, referenced downstream by persistent name.
Each other section becomes true as C1 lands, and the commit that builds it
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

The generation, bumped by every applied command, is the authority's
runtime state beside the document (§Commands and undo), so an undone
document equals the one before it. Built so far (`arrix_doc::Document`):
parameters and parts; `schema` is the build's constant, and plugin data
and `meta` join with their first use.

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
`RecordId`, and `CurveKey`, the key of a profile curve (§Persistent
naming). Its text is 13 characters of Crockford's alphabet, most
significant first, so the first is `0`–`F`; written upper-case, read in
either case with Crockford's aliases (`I` and `L` as 1, `O` as 0); a JSON
string both ways. An `IdMinter` (SplitMix64) mints them from a seed its
caller passes: `arrix-core` reads no entropy source, so the same seed
gives the same ids on every target, a `const` assertion pinning the first
ids of seed 1 in every build, the wasm one included. A client's seed is
supplied by its caller: the session never mints, `arrix-app` and
`arrix-cli` read entropy at the edge when they first build commands, and
tests pass a fixed seed.

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

The grammar, from loosest to tightest: `+ -`, then `* /`, then a prefix
`-`, then `^` (right-associative, so `-2^2` is −4 and `2^3^2` is 512),
then atoms: a number with an optional unit, a call, a name, a
parenthesised expression. **A unit is only ever a number's suffix**
(`2 m`, `12mm`), so a bare `m` is the parameter named `m`. The functions:
`sin`, `cos`, `tan` (of an angle or a plain number, as radians), `asin`,
`acos`, `atan`, `atan2` (to an angle), `sqrt`, `abs`, `min`, `max`, and
`round`, `floor`, `ceil` of a plain number only, so rounding a length is
written `round(w / 1 mm) * 1 mm`. A dimension is the exponents of length,
angle and mass; a dimensioned value is raised only to a whole number
written out. `Expr::check` is the dimensional analysis, against the
parameters' kinds; `Expr::evaluate` gives the SI value; `Expr::quantity`
does both for a slot of a known kind, and a count must come out whole.
Each failure is a code: `expr.syntax` (with the byte it stopped at),
`expr.unknown-unit`, `expr.unknown-name`, `expr.unknown-function`,
`expr.arity`, `expr.dimension`, `expr.unit-mismatch`, `expr.not-finite`,
`expr.not-whole`. An `Expr` serialises as its text and is parsed on the
way in.

## Features

A feature is one history entry. Built-in and plugin features have **one
record shape**; that is the point of C1 (`SEED.md` §7).

```rust
pub struct FeatureRecord {
    id: FeatureId,
    type_id: FeatureTypeId,        // "core.extrude", "gears.spur": namespaced, stable
    type_version: u32,             // the feature type's own schema version
    name: String,                  // what the tree shows; unique within the part
    params: BTreeMap<String, Expr>,     // the form's numeric fields, each an expression
    choices: BTreeMap<String, String>,  // the form's plain values: a word the type lists
    inputs: BTreeMap<String, Ref>,      // named inputs: a plane, a profile, faces, a body
    suppressed: bool,
    sketch: Option<Sketch>,        // a core.sketch feature's sketch, §Sketches
    frozen: Option<FrozenResult>,  // §Frozen results; always present for a plugin feature
}
```

A form field the record leaves out takes the type's default; a field the
type does not list is refused (`feature.unknown-field`), never ignored.
`choices` is omitted from JSON when empty, and `sketch` when absent.

The feature *type* (its parameters' form, its inputs, its `evaluate`) comes
from a registry. Built-in types register under the reserved `core.`
namespace through the same `FeatureType` shape a plugin's `Feature` is
adapted onto (`docs/PLUGINS.md` §Features), so the evaluator has one code
path for both: `FeatureType` is the plugin API's `evaluate` for one type,
taking the plugin API's `Kernel` and a `FeatureArgs` of SI parameters, the
record's choices and resolved inputs. A `type_id` the registry does not
know is not an error: the feature evaluates as frozen. Until frozen
results land it fails as `feature.unknown-type`, and a `type_version`
other than the registered one as `feature.type-version`.

A feature's outputs are **slots**: named bodies, datums (plane, axis,
point, frame) and sketches. `core.extrude` in new-body mode has one body
slot, `body`; in cut or join mode it has none of its own and modifies the
target's slot (§Bodies across features).

### The core feature types

| Type | Inputs | Kernel operation | Cycle |
|---|---|---|---|
| `core.sketch` | a plane: a datum, a planar face, or a frame | none in the kernel; `arrix-sketch` solves it; its closed regions are profiles | C1 |
| `core.datum-plane` | offset from a plane or face, or from a world plane; through three points, at an angle about an edge | none | C1 |
| `core.extrude` | a profile (sketch regions), a direction, a mode: new body, join, cut | `ops::extrude`, then `fuse` or `cut` | C1 |
| `core.revolve` | a profile, an axis, a mode | `ops::revolve`, then `fuse` or `cut` | C1 |
| `core.fillet`, `core.chamfer` | edges of one body | `ops::fillet`, `ops::chamfer` | C1 |
| `core.hole` | points on a planar face (sketch points), diameter, depth or through, counterbore / countersink | a revolved tool profile, then `cut` | C1 |
| `core.pattern-linear`, `core.pattern-circular` | features or a body, count, spacing | `ops::transform` of the tool or body, then `fuse`/`cut` | C1 |
| `core.boolean` | a target body, tool bodies, an operation | `fuse`, `common`, `cut` | C1 |
| `core.mirror` | features or a body, a plane | `ops::mirror` (Arris ask A11) | C1 once A11 is released, else the first cycle after |

`core.datum-plane` is built in its offset form: `offset` (a length,
default 0) along the normal of its `plane` input, a datum's `plane` slot
or a planar face by persistent name (its outward normal). With no input
it stands on the world plane its `world` choice names: `xy` (the
default), `yz` or `zx`, `Frame::WORLD_*`'s planes. Both at once is
`datum-plane.two-bases`. Its output slot is `plane`. No `core.origin`
type: a world plane is a choice, not a feature.

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
the rest are by id. A `SlotName` is one lower-kebab word (`body`,
`plane`), the grammar of a diagnostic code's segment. In JSON a `Ref` is
an object with one key, the variant in snake case: `{"topo":"face:…"}`,
`{"slot":{"feature":"…","slot":"body"}}`.

## The dependency DAG

The DAG exists from day 0 (`SEED.md` §6.5). Its nodes are parameters,
features, parts and plugin records; its edges are every `Ref` in a record
and every parameter name in an expression (a sketch dimension's
included, by the field `sketch.<constraint>`), and each feature reads its
part (whose order and rollback place it). A `Topo` reference reads every
feature its name's root and steps name. It is derived from the document,
never stored (`arrix_doc::Dag`, rebuilt whole by every command for now;
incremental when a measured budget asks). A reference whose target the
document lacks is no edge: it is the evaluator's lost reference.
Plugin-record nodes join with plugin data; the implicit edges of a body
slot's versions (a feature reading the body a join or cut before it made)
join with join and cut.

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
- **The order is deterministic**: parameters by id, then parts, then
  features by part and history position, each as soon as what it reads
  is placed. A refused order names the feature, the field (`inputs.plane`,
  `params.width`) and the target (`dag.order`); a cycle is returned from
  its least node, each node reading the next (`dag.cycle`).
  `Document::validate` checks these with the rest of a document's
  invariants: parameter names are identifiers and unique, feature names
  unique within a part, a part's history lists exactly its features, and
  its rollback is within it.

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

**As built** (`arrix_doc::Evaluator`; `LocalSession` runs it on its
worker, where `evaluate_while` hands over each feature's event as it is
made and stops between features for a newer snapshot,
`docs/CONCURRENCY-WASM.md` §The evaluator). Parameters evaluate in the DAG's order, then every
feature. A feature at or after its part's rollback index is `rolled-back`,
a suppressed one `suppressed`; neither has outputs. The input hash covers
the Arris version, the feature's id (names are rooted at it, so two
features never share a result), `type_id`, `type_version`, the plugin's
version (none for a built-in), each parameter's
SI value (not its text: `1 cm` and `10 mm` hash alike), the choices and
each input as the geometry it resolved to (a plane as its frame); the
feature's name is not an input. It is canonical JSON through BLAKE3,
written as 64 hex digits. A failure's codes: `input.unavailable` (a read
feature or parameter has no value; its `refs` name it), `ref.lost` (with
candidates), `input.wrong-kind`, `feature.output` (a type filled its
slots other than it declared), `expr.*`, `kernel.*`. A kernel failure
keeps its `KernelCall` in the event. Each feature's outcome is an
`EvalEvent { feature, outcome }`, the outcome tagged by `status` (`ok`
with its hash, whether it was cached and its slots as plain data;
`failed`; `suppressed`; `rolled-back`), and crosses as JSON.

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
    kind: TopoKind,           // Face | Edge | Vertex
    root: NameRoot,           // where the chain starts
    chain: Vec<NameStep>,     // what happened to it since, feature by feature
}

pub enum NameRoot {
    Sweep { feature: FeatureId, part: SweepPartName },   // a cap, or a part of curve k
    Plugin { feature: FeatureId, key: Id },              // Arris ask A1, consumer roles
    Frozen { feature: FeatureId, index: u32 },           // a frozen result's stored name table
    // Primitive and Imported (C2) join with primitives and import.
}

pub enum SweepPartName {      // curve-keyed parts name the profile curve by its CurveKey
    StartCap, EndCap, Side(CurveKey), StartEdge(CurveKey), EndEdge(CurveKey),
    Rise(CurveKey), StartVertex(CurveKey), EndVertex(CurveKey), Cavity(CurveKey),
}

pub enum NameStep {
    Modified { feature: FeatureId, split: u32 },         // a piece of it; split 0 when whole
    Generated { feature: FeatureId, from: Vec<PersistentName> }, // e.g. an edge from two faces
}
```

**Text form.** A name has one canonical text, which is how it is written
in a document, in a diagnostic and across the plugin boundary (the WIT
world's `persistent-ref`):

```text
name  := kind ':' root ('/' step)*
kind  := 'face' | 'edge' | 'vertex'
root  := 'sweep.' ID '.' part | 'plugin.' ID '.' ID | 'frozen.' ID '.' N
part  := 'start-cap' | 'end-cap' | curve-part '.' ID
step  := 'mod.' ID '.' N | 'gen.' ID '[' name (',' name)* ']'
```

`ID` is an id's 13 characters, `N` a decimal `u32` without leading zeros,
`curve-part` one of `side`, `start-edge`, `end-edge`, `rise`,
`start-vertex`, `end-vertex`, `cavity`: `face:sweep.<feature>.side.<curve>`.
A parse error names the byte it stopped at; `gen` nests at most 32 deep.

- **Roots** come from what made an entity from nothing. A sweep's side
  face is rooted at the **key of the curve** that swept it, not at its
  loop index: every curve of an `arrix_core::Profile` carries a
  `CurveKey`, the sketch entity's id for a sketch region, a key the
  plugin chooses for a plugin's own profile. A curve that is piece `i` of
  an entity the arrangement cut (pieces numbered from 0 along it) is keyed
  `arrix_sketch::curve_key(entity, i)`, the id mixed with the piece
  number, so an entity that bounds one region twice gives two keys and an
  uncrossed sketch keys every curve by its entity id alone. Cutting a
  whole entity re-keys it, and what named its face is a lost reference. `arrix-kernel` translates
  keys to Arris's `(loop_index, segment)` on the way in and back on the
  way out, so re-ordering or re-drawing a sketch keeps names on the curves
  that survive. A part at a vertex (`rise`, `start-vertex`, `end-vertex`)
  is named by the curve that starts there.
- **Generated entities** name their origins: the rim of a hole is the edge
  generated from the pair *(hole wall, top face)*. Edges and vertices
  derive from their faces where Arris records them that way; a sweep
  records its own edges and vertices by role (`start-edge`, `end-edge`,
  `rise`, `start-vertex`, `end-vertex`), so theirs are roots, not `gen`
  steps. An extrude of a plate with a bore names 7 faces, 15 edges and 10
  vertices, all from roots (`crates/arrix-kernel/tests/probe.rs`).
- **Kept entities** keep their names across features; a kept id is not a
  step.
- **Plugin features get naming for free** when they build through kernel
  operations. Topology a plugin builds directly is rooted at a consumer
  role with a key the plugin chooses (ask A1). `gears.spur` keys each
  curve by its tooth, so a teeth edit keeps the surviving teeth's faces'
  names and loses the rest's.

**Resolution.** A `Ref::Topo` resolves against the current evaluation to
exactly one entity or to a diagnostic. A name that matches nothing, or
more than one entity, is a **lost reference**: the feature fails with a
diagnostic that lists candidates, ranked by the longest shared chain
prefix, and the user (or an agent) re-picks by command. As built: the
pool is the bodies of the features the name reads; candidates are that
pool's names of the same kind, the same root first, then the same
feature's same kind of sweep part (another side face), then the same
feature, then the longer shared chain, ties in name order, at most five;
`crates/arrix-cli/tests/gear_naming.rs` holds this through `gears.spur`.
It is **never rebound silently** (`SEED.md` §8.2), and never matched by centroid,
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
    AddParam { param: ParamId, record: Param },
    SetParam { param: ParamId, expr: Expr },
    DeleteParam { param: ParamId },
    AddPart { part: Part },                               // with its features: DeletePart's inverse
    DeletePart { part: PartId },
    AddFeature { part: PartId, at: usize, record: FeatureRecord },
    EditFeature { feature: FeatureId, edit: FeatureEdit },
    DeleteFeature { feature: FeatureId },
    ReorderFeature { feature: FeatureId, to: usize },    // `to` counted after the move
    SetRollback { part: PartId, at: Option<usize> },
    SketchEdit { feature: FeatureId, edit: SketchEdit },  // §Sketches
    Group { label: String, commands: Vec<Command> },       // one undo entry
    // PluginData { plugin, edit } joins with plugin document data.
}

pub struct SketchEdit {         // arrix_sketch; what it leaves out is kept
    points: BTreeMap<PointId, Option<Point>>,             // null removes the record
    entities: BTreeMap<EntityId, Option<Entity>>,
    constraints: BTreeMap<ConstraintId, Option<ConstraintRecord>>,
    construction: BTreeMap<EntityId, bool>,
}

pub struct FeatureEdit {        // what it leaves out is kept
    name: Option<String>,
    suppressed: Option<bool>,
    params: BTreeMap<String, Option<Expr>>,   // null removes the field
    choices: BTreeMap<String, Option<String>>,
    inputs: BTreeMap<String, Option<Ref>>,
}
```

In JSON a command is an object with one key, the variant in snake case,
and an `Expr` is its text: `{"set_param":{"param":"…","expr":"12 mm"}}`.
Inserting a feature before a part's rollback index moves the index with
the features after it; inserting at the index puts the feature behind the
bar. Deleting the last feature before the index moves it back, so that
deletion's inverse is a `Group` of the insertion and the old index.

Applying a command is a pure function of the document and the command:
it returns the new document, its **inverse** command, and the set of nodes
it touched. It either applies whole or not at all.

- **Generations.** The authority (`arrix_doc::Authority`, the document's
  one writer) stamps every DAG node (§The dependency DAG) with the
  generation that last changed it. A command applies if none of the nodes
  it touches (the parameter, the part, the feature it adds, edits, moves
  or deletes) carries a stamp newer than its `base`; otherwise it is
  rejected as **stale**, naming them, and the author rebuilds it against
  the new state. There is one writer, so there is no merge and no CRDT.
  Locally the base is always current. A deleted node keeps its stamp.
- **Undo is per author, by inverse commands.** Each author has an undo and
  a redo stack of inverses. Undo applies the top inverse as an ordinary
  command, subject to the same staleness rule against the generation the
  entry's change made; an undo that would clobber another author's later
  change is stale and refused, and the entry stays. Undo and redo put the
  stamps of what they touch back to what they were before the entry's
  change: the content is back, so its stamp is, and the author's next
  entry stays fresh. Undo never restores a snapshot. A new command clears
  its author's redo stack; an undo or redo whose effect equals the
  current state is consumed as a no-op.
- **One gesture is one command** (`SEED.md` §8.2). A multi-step gesture
  (a sketch drag that adds a point and a coincidence) is one `Group`. A
  command whose effect equals the current state is not applied and leaves
  no undo entry.
- **Plugin commands** are composed from core commands and the plugin's own
  `PluginData` edits (`docs/PLUGINS.md` §Commands), below.

**Plugin commands** (ADR-0004) run on the client that issues them: the
plugin expands a gesture into one `Group` of core commands and
`PluginData` edits, which is what is submitted, and its undo entry is
that group's inverse, computed by the core. A `PluginData` edit is a
whole-record put or remove, so the core inverts it without the plugin.
A plugin never supplies an inverse, and the authority never runs plugin
code, so undo works with the plugin absent.

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
- **Edits** are `SketchEdit` commands: whole-record puts and removals of
  points, entities and constraints, and construction marks, keyed by the
  ids their author minted. The inverse holds the records each entry
  replaced. The sketch is checked whole after an edit, as one read from a
  file is: a removal must name what stands on what it removes
  (`SketchEdit::remove` collects it), or it is refused, never swept. The
  client solves and the edit carries the positions it produced; the
  authority applies it without solving, and the evaluator re-solves from
  the stored positions (ADR-0005). A client makes one command of a
  gesture by diffing its draft before and after (`SketchEdit::diff`).
- A dimension's expression travels on its constraint record as text, so
  undo and removal carry it; a document whose expression does not parse
  is malformed (`doc.invalid`), and the parameters it names are DAG edges.
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
- **What opens and saves today** (`arrix_doc::open`, `arrix_doc::save`):
  the directory form, as `document.json` (`schema` 1; nothing else in the
  header is interpreted yet), `params.json` (the parameters by id; absent
  reads as none) and `parts/<part-id>.json` (one part, its id matching the
  file's name). `arrix_doc::to_json` is the writer: it goes through
  `serde_json::Value`, whose map is ordered, for sorted keys. A record
  with a field this build does not know is refused, never dropped on the
  next save. The opened document is checked as a command's result is
  (`Document::validate`). A newer schema is refused with a message naming
  both (`the document's schema is 2, newer than this build's 1`), a
  schema below 1 as one no ArriX wrote. `save` returns the files by
  path; the caller writes them and removes what an earlier save left.
  Every command undone restores the earlier save's bytes, and redone the
  later one's (`crates/arrix-doc/src/save/tests.rs`).
  `tests/docs/empty/` is the empty document; `tests/docs/gear-on-plane/`
  is M1's first scenario, written by the commands of
  `crates/arrix-cli/tests/gear_on_plane.rs` and checked against them on
  every run (`UPDATE_SNAPSHOTS=1` rewrites it).
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

None open. Undo granularity for plugin commands was settled by ADR-0004
(§Commands and undo).
