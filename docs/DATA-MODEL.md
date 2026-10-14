# Data model

What a document is, how it changes, how it evaluates, how its geometry is
named and how it is written to disk. The charter is `SEED.md` §6.4–§6.5;
this document is the design those sections commit to. Built, as of C1:
everything below but plugin-data edits (§Commands and undo) and the
plugin-record nodes, which wait for plugin document data (not scheduled;
`docs/BACKLOG.md`).

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

Assemblies (ADR-0029) extend this type with `assemblies: BTreeMap<AssemblyId,
Assembly>`: `Assembly { id, name, components, joints }`; `Component { id,
name, of: Part | Assembly, pose: Option<Pose> }` (a pose grounds it);
`Joint { id, name, kind: Fixed | Revolute | Prismatic, parent, child:
Anchor, value, offset: Expr, limits: Option<[Expr; 2]>, flip }`; `Anchor {
path: Vec<ComponentId>, axis, origin: Option<PersistentName> }`; `Pose` a
position and URDF's roll, pitch, yaw, all expressions. Built so far
(`arrix_doc::Assembly` and kin, `Document::assemblies`): the stored types,
their file form and `Document::validate` (ids match their keys, names are
unique within their scope, a component's part or assembly exists, a joint's
first path component is its assembly's). A `FeatureRecord` also holds
`file: Option<ImportedFile { name, blob }>`, the imported file's base name
for display (never a path) and its blob. The commands (§Commands and
undo), the DAG's nodes (§The dependency DAG) and the assembly pass
(§Evaluation) are built, and the eval line and the protocol carry an
assembly's outcome (`Assembled`).

Materials (ADR-0031) add `materials: BTreeMap<MaterialId, Material>` to the
document (`Material { id, name, density: Expr, appearance: Rgb }`) and
`material: Option<MaterialId>` and `body_materials: BTreeMap<SlotRef,
MaterialId>` to a `Part` (`SlotRef` is a feature and a body slot, written
`<feature>.<slot>`); mass is evaluated from them, never stored. Built so
far (`arrix_doc::Material`, `Document::materials`): the stored types, their
file form (schema 5, `materials/<id>.json`), `Document::validate` (ids
match their keys, names are unique, a colour is in 0 to 1, a density
parses, an assignment names a material and a feature of the part), the
four commands (`AddMaterial`, `EditMaterial`, `DeleteMaterial`, refused
while a part is made of it as `material.in-use`, `AssignMaterial`, each
with its inverse) and a part's DAG edge to its materials, a material's to
the parameters its density reads. The mass pass is built (§Evaluation). A STEP's
product structure imports as parts, an assembly and grounded components in
one `Group` (ADR-0032), not built yet.

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
    inputs: BTreeMap<String, Input>,    // named inputs: a plane, a profile, faces, a body; a Ref, or a list of them
    suppressed: bool,
    sketch: Option<Sketch>,        // a core.sketch feature's sketch, §Sketches
    frozen: Option<FrozenResult>,  // §Frozen results; always present for a plugin feature
}
```

A form field the record leaves out takes the type's default; a field or a
choice the type does not list is refused (`feature.unknown-field`), never
ignored. Every choice a type declares reaches the feature (the record's
word or the type's default; a choice with no default and no word is not
given), and a word it does not list is `feature.choice`, one check in the
evaluator for every type (ADR-0018); the input hash covers the choices as
the record wrote them. A parameter or an input may be shown only under a
choice's values (`shown_when`): a form matter alone, a hidden field is
still resolved and keeps its value.
An input that takes a list (a fillet's edges, a hole's points), and the
`edge`, `point`, `axis` and `tool` input kinds, are ADR-0012's.
`choices` is omitted from JSON when empty, and `sketch` when absent.

The feature *type* (its parameters' form, its inputs, its `evaluate`) comes
from a registry. Built-in types register under the reserved `core.`
namespace through the same `FeatureType` shape a plugin's `Feature` is
adapted onto (`docs/PLUGINS.md` §Features), so the evaluator has one code
path for both: `FeatureType` is the plugin API's `evaluate` for one type,
taking the plugin API's `Kernel` and a `FeatureArgs` of SI parameters, the
record's choices and resolved inputs, and a `core.sketch` record's sketch
with its dimensions resolved. It returns the plugin API's output plus the
slots of a kind the plugin API has no word for, which only a built-in
declares (`sketch_slots`) and fills: a sketch's. A `type_id` the registry does not
know is not an error: the feature evaluates as frozen. Until frozen
results land it fails as `feature.unknown-type`, and a `type_version`
other than the registered one as `feature.type-version`. The registry
refuses a type whose slot `modifies` anything but one of its own body
inputs, or is not a body slot (§Bodies across features).

A feature's outputs are **slots**: named bodies, datums (plane, axis,
point, frame) and sketches. `core.extrude` in new-body mode has one body
slot, `body`; in cut or join mode it has none of its own and modifies the
target's slot (§Bodies across features).

### The core feature types

| Type | Inputs | Kernel operation | Cycle |
|---|---|---|---|
| `core.sketch` | a plane: a datum's plane slot or a planar face; without one, a world plane | none in the kernel; `arrix-sketch` solves it; its closed regions are profiles | C1 |
| `core.datum-plane` | offset from a plane or face, or from a world plane; through three points, at an angle about an edge | none | C1 |
| `core.extrude` | a profile (sketch regions), a direction, a mode: new body, join, cut | `ops::extrude`, then `fuse` or `cut` | C1 |
| `core.revolve` | a profile, an axis, a mode | `ops::revolve`, then `fuse` or `cut` | C1 |
| `core.fillet`, `core.chamfer` | edges of one body | `ops::fillet`, `ops::chamfer` | C1 |
| `core.hole` | points on a planar face (sketch points), diameter, depth or through, counterbore / countersink | a revolved tool profile, then `cut` | C1 |
| `core.pattern-linear`, `core.pattern-circular` | features or a body, count, spacing | `ops::transform` of the tool or body, then `fuse`/`cut` | C1 |
| `core.boolean` | a target body, tool bodies, an operation | `fuse`, `common`, `cut` | C1 |
| `core.mirror` | features or a body, a plane | `ops::mirror`, then `fuse`/`cut` | C1 |
| `core.import-step` | none; the record's file | `Kernel::import_step` (`step::read`, metres) | C2 |

`core.extrude` is built: a `region` input (a sketch's region by key), a
signed `distance` (default 10 mm, along the profile plane's normal), a
`mode` choice, `new` (the default), `join` or `cut`, and a `target` body
input, set in the last two. `join` fuses the extruded solid into the
target and `cut` removes it. Its one output slot, `body`, names `target`
as the slot it modifies, so with `target` set it is the target slot's
next version, and with `target` unset (mode `new`) it is a slot of its own
(§Bodies across features). A `join` or `cut` without a target fails as
`extrude.no-target`, a `new` with one as `extrude.stray-target`, a mode
that is none of the three as `feature.choice`, no region as
`extrude.no-region`, and a target that is not a body as
`input.wrong-kind`.

`core.fillet` and `core.chamfer` are built: a `target` body input, an
`edges` list of edge inputs (`Ref::Topo` edges of the target, ADR-0012),
and a `radius` (fillet) or a `distance` (chamfer), a length defaulting to
1 mm. The slot, `body`, modifies `target`. Naming one edge of a tangent
chain blends the chain, so listing one edge or all of them gives the same
body (§Persistent naming, through a blend). They fail as `fillet.no-target`
or `chamfer.no-target`, `fillet.no-edges` or `chamfer.no-edges`, an edge
no body the feature reads holds as `ref.lost` with candidates, and a
blend no face can hold as `kernel.degenerate.blend-too-large` with the
call kept, which `arrix eval --save-fixtures` writes as a recipe
(`tests/docs/oversize-fillet/`). A feature that blends keeps no tool, so
a pattern or a mirror of it is refused.

`core.revolve` is built: a `region` input, an `axis` input (a sketch line,
from its start along its length, or a straight edge, ADR-0012), an `angle`
(default 360 deg, positive and at most a full turn; a full turn has no end
caps), the `mode` choice, `new` (the default), `join` or `cut`, and the
`target` body input of `core.extrude`, with the same slot rule, tool
keeping (ADR-0013) and codes under `revolve.`: `revolve.no-region`,
`revolve.no-axis`, `revolve.angle`, `revolve.no-target`,
`revolve.stray-target`. An axis through the region, or one
the kernel refuses, fails as the kernel's own `kernel.*` code with the call
kept. Its turn counts in the feature's sweep numbering (ADR-0011).

`core.hole` is built: a `target` body input and a `points` list of
sketch points (a vertex has no plane to drill from: `hole.no-normal`),
a `diameter`, the `extent` choice, `blind` (the default, to `depth`) or
`through`, and the `kind` choice, `simple` (the default), `counterbore`
(`cb-diameter`, `cb-depth`) or `countersink` (`cs-diameter`, the included
`cs-angle`). Each hole is one tool revolved about the inward normal of its
point's sketch plane, starting 1 mm outside it; a through hole's tool is
1 m long, the kernel having no extent query for a body to ask. The tools
are cut in the list's order, one boolean each (ADR-0014), so a hole's
faces are named by its place in the list: moving a point renames
nothing, adding one at the end renames no other hole, removing one
renames the later holes' roots. The tools are then fused and kept for a
pattern or a mirror to copy (ADR-0013). A hole that removes nothing is
`hole.misses`; the other codes are `hole.no-target`, `hole.no-points`,
`hole.diameter`, `hole.depth`,
`hole.counterbore` and `hole.countersink`. A hole across a fillet is the
Arris gap in `docs/BACKLOG.md`.

`core.pattern-linear` and `core.pattern-circular` are built: a `target`
body input, a `tools` list of tool inputs (ADR-0013), a `count` (whole,
2 to 1000: the source's own work is instance 0, so a count of 1 copies
nothing, `pattern.count`) and, linear, a `spacing` along its `direction`
(an axis input: a sketch line or a straight edge, `pattern.no-direction`,
`pattern.spacing` for zero), circular, an `angle` about its `axis`
(`pattern.no-axis`, `pattern.angle`). A full turn is shared out among the
instances; a part of one runs from the first instance to the last. Each
tool's copies, instances 1 up, are applied to the target one at a time
with the tool's own operation, tool by tool, copy by copy, one boolean
each (ADR-0014); with no tools the target's own copies are fused into it,
a pattern of a body. Fusing a source's copies into one tool and applying it
once would number that boolean by the count, and raising the count would
rename every copy that stays; applied one by one, a copy added at the end
renames none, and a copy's wall is
`…/inst.<F>.<i>/gen.<F>.<i-1>[<wall>/inst.<F>.<i>]`. A copy that
cuts nothing, or fuses to nothing it touches, fails as `pattern.misses`.
A pattern keeps no tool.

`core.mirror` is built: a `target` body input, a `tools` list of tool
inputs (ADR-0013) and a `plane` input (a datum plane, a sketch plane or a
planar face, as a frame; its origin and normal are the mirror's). It is a
pattern of one copy, through `Kernel::mirror` (instance 1 of the feature):
each tool's reflection is applied to the target with the tool's own
operation, or, with no tools, the target's reflection is fused into it.
The codes are `mirror.no-target`, `mirror.no-plane` and `mirror.misses`
(a reflection that cuts nothing, or fuses to nothing it touches).

`core.import-step` is built (ADR-0029, type version 2 since ADR-0032): no
inputs, a `frame` choice, `world` (the default, version 1's only
behaviour: the solid where the file's assembly places it) or `product` (in
its product's own frame), a `solid` count
parameter (which solid of the file, from 1 in the reader's order, default
1) and one output slot `body`, of its own. The file is the record's
`file`, a blob in the document that the input hash holds by name, so a
different file is a different result. It reads through `Kernel::import_step`
(not the plugin API's `Kernel`: the built-in is handed a `FileKernel`, so
`arrix-plugin-api` is unchanged), in metres whatever the file declares, and
every face, edge and vertex is named `import.<feature>.<entity>[.<instance>]`
from the file's own entity (§Persistent naming). The codes are
`import.no-file` (the record keeps none), `import.no-solid` (a `solid`
that is not a whole number from 1, or past the file's count) and
`kernel.import.<kind>` (the file or solid is refused, its message naming
the file entity). It is built in, so never frozen. No create command
lists it yet: only an import creates it, with its file (M2). An import of
a whole file is `import_products` (ADR-0032 §2): given the document, the
file, the `StepStructure` and an id minter it returns one `Group` (an undo
removes it all) and the roots it made. A product is a part with one
`product`-frame import feature per solid (a product placed twice, one
part), a product with children an assembly, each occurrence a component
grounded at the file's placement (metres and roll, pitch, yaw in radians),
each distinct colour a material named `Colour #rrggbb` with no density.
Names are made unique against the document (`arm 2`); a solid the reader
refused is left out and said (`import.refused`), a placement it cannot read
is `import.placement`, a file with nothing importable `import.empty`.
`arrix import <step> -o <dir>` writes it as a new document.

`core.boolean` is built (type version 2): a `target` body input, a `tools`
list of body inputs and an `op` choice, `fuse` (the default), `cut` or
`common`. Each tool is one kernel call on the result of the one before,
in the list's order, each a boolean of the feature's own number
(ADR-0014); `common` of several tools is the target's intersection with
each in turn. Its one output slot, `body`, modifies `target`, so it is the
target slot's next version; `tools` modifies nothing, so every tool's slot
ends there (a later reader gets `slot.consumed`). A missing target, an
empty `tools` or an input that is not a body fails as `boolean.no-body`, an
`op` that is none of the three as `feature.choice`, a lone reference where
the list goes as `input.list`. Version 1 had one `tool` input; `open`
migrates it to a `tools` list of one (`arrix_doc::migrate::core_types`,
run after the schema chain on every open), and a version-1 record handed
to the evaluator unmigrated fails as `feature.type-version`.

`core.datum-plane` is built in its offset form: `offset` (a length,
default 0) along the normal of its `plane` input, a datum's `plane` slot
or a planar face by persistent name (its outward normal). With no input
it stands on the world plane its `world` choice names: `xy` (the
default), `yz` or `zx`, `Frame::WORLD_*`'s planes. Both at once is
`datum-plane.two-bases`. Its output slot is `plane`. No `core.origin`
type: a world plane is a choice, not a feature.

`core.sketch` stands on its plane the same way: its `plane` input, or the
world plane its `world` choice names (`sketch.two-bases`; a word that is no world plane is
`feature.choice`). It has no form parameters; its record's `sketch` is its
content. Evaluating it resolves each driving dimension's expression to
the quantity its constraint measures (a failure names the constraint as
`Ref::Sketch`; an expression on a constraint with no value is
`sketch.not-a-dimension`), solves from the stored positions (ADR-0005),
and fills its one slot, `sketch`: the plane, the degrees of freedom left,
the redundant constraints, the solved sketch, and every region, largest
first, as its `RegionKey` and its keyed `Profile` on the plane. A
conflict fails the feature as `sketch.conflict` naming the constraints
that cannot be met together, a solve that stopped short as
`sketch.not-converged` naming the ones it left unmet; nothing is
written back to the document.

Sweep, loft, shell, draft and split wait for their Arris cycles
(`SEED.md` §6.2). A type that is not in this table is not planned around.

### Bodies across features

A part's bodies are named slots that features create, modify and consume
(ADR-0007). A body is referenced as *slot `s` as of feature `f`*: after an
extrude in cut mode, the body it cut is a new version of the same slot.

- **A body input** (input kind `body`) is written as `Ref::Slot` naming
  the feature that *made* the slot, and resolves to the version current
  at the reading feature's place in the history: the output of the last
  feature before it that modifies the slot, else the maker's. Inserting a
  modifier earlier moves every later reader with it, and no reader's
  record changes.
- **A modifying slot** is an output slot whose spec names one of the
  type's body inputs as the one it `modifies`. Its body is the target
  slot's next version, not a slot of its own: readers after it name the
  target's slot, and `Ref::Slot` on the modifier's own slot names
  nothing.
- **A consumed slot**: a body input that no output slot modifies is a
  tool, and ends its slot. A later feature reading it by body input
  fails as `slot.consumed`, naming the consumer; a persistent name into it
  follows the consumer (below).
- A `Topo` name resolves against the version of its slot current at the
  reader, so a face kept through a join is found after it.
- **A name into a consumed slot follows its consumer** (ADR-0020): it is
  looked for in the slot the consumer modifies, as of the reader, so an
  edge of a joined gear the join left alone is filleted downstream, live
  or frozen. A name not found there, or a consumer that modifies no slot,
  is `slot.consumed`; a body input on a consumed slot always is. The DAG
  gives the reader the consumer and the features touching the slot it
  follows into.
- **A modifying slot is a version only while its input is set.** A
  type whose output `modifies` a body input, with that input unset in the
  record, has the slot as its own: `core.extrude` in mode `new` makes a
  body that later features name as `Ref::Slot` on the extrude. Set, it is
  the target's next version and names nothing of its own. The record
  decides, so a feature that fails with the input set still counts as a
  modifier (its readers are unavailable), and one whose record names no
  target does not.
- **As built**: a reader's input hash covers the body it read as the
  input hash of the feature that produced that version and the slot
  (evaluation is deterministic, so that identifies it). A modifier or consumer that is suppressed is not
  there; one that failed leaves the reader unavailable
  (`input.unavailable` naming it), never on an earlier version. A body is
  read within its part: naming one of another part is `input.cross-part`
  except through a joint's anchor, which reads the final state of the
  part it names (ADR-0029).

- **The tool a feature keeps** (ADR-0013, as built): a feature that joins
  or cuts hands back the tool it applied, and the evaluator holds it beside
  the feature's slots, in its cache entry and in the state later features
  read. It is no part body and no slot: nothing reads it by `Ref::Slot`
  and nothing consumes it. A `tool` input (a `Ref::Feature`) reads it, as
  a body handle of the reader's own and the operation, `fuse` or `cut`;
  its input hash is the source feature's. A feature that keeps none, or a
  frozen plugin feature (a stored result holds no tool), is `input.no-tool`
  naming it; a tool of another part is `input.cross-part`.
  `core.extrude` in `join` and `cut` modes keeps its tool.

Built-ins and plugins declare these the same way, through the plugin
API's `input-kind.body` and `slot-spec.modifies` (`docs/PLUGINS.md` §One
interface).

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
    Region { feature: FeatureId, key: RegionKey },         // a sketch region, ADR-0006
    Plugin { plugin: PluginId, record: RecordId },         // a plugin document record
}

pub struct RegionKey {
    entities: Vec<SketchEntityId>,   // every entity bounding the face, sorted
    sample: [i64; 2],                // a point inside it, in nanometres of (u, v)
}
```

Every `Ref` is a DAG edge (each element of a list input, ADR-0012). A
`Ref::Feature` in a `tool` input reads the tool a joining or cutting
feature keeps (ADR-0013). `Topo` references are the ones naming solves;
the rest are by id. A `SlotName` is one lower-kebab word (`body`,
`plane`), the grammar of a diagnostic code's segment. In JSON a `Ref` is
an object with one key, the variant in snake case: `{"topo":"face:…"}`,
`{"slot":{"feature":"…","slot":"body"}}`.

A joint's `Anchor` (ADR-0029) is the one reference that crosses parts: a
path of components down to a part, and the `PersistentName` of an axis face
or edge (and optionally an origin face) in that part's final bodies. It
resolves or is `ref.lost` with candidates on the joint, never re-bound.

A record holds an `Input` under each input name: one `Ref`, or a list of
them (`Input::One`, `Input::Many`, an object or an array in JSON, so a
record from before lists reads unchanged). An empty list is the input
unset, and the order of a list is kept. The record, `FeatureEdit` (which sets a list whole, its inverse the
earlier value) and the DAG (one edge per element) carry lists. An
`input-spec` says whether its input takes one (plugin API 0.4), and a list
on an input that does not, or one reference on an input that does, is
`input.list`.

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
Plugin-record nodes join with plugin data.

**The implicit edges of a body slot's versions** (ADR-0007): a feature
that reads slot `s`, by a body input or by a `Topo` name whose root or
steps name a feature whose body is a version of `s`, reads every feature
before it in its part that modifies or consumes `s` (a consumer, since
the reader's outcome, `slot.consumed`, depends on it). They are derived
from the history like the rest, and point only backwards within a part,
so they add no cycle and no order error. They need to know which feature
types modify what, so `Dag::build_with(doc, registry)` adds them and the
evaluator uses it; `Dag::build` and `Document::validate` stay
registry-free and hold the explicit edges alone.

**Assembly nodes** (ADR-0029 §10; `arrix_doc::Node::{Assembly, Component,
Joint}`): a component reads its part or nested assembly and the
parameters of its pose; a joint reads the parameters of its `value`,
`offset` and `limits`, the components each anchor's path goes through
(`dag::anchor_target`), the anchored part (its order and rollback place
what a name resolves against), and the features a name resolves against:
those its root and steps name, and, with the registry, every feature
before the rollback that modifies or consumes the body slot its root made
(the part's final state of it, which the last of them reads through the
versions above). A feature after them, and any feature of a part nothing
anchors to, is no edge. A path that leads nowhere is no edge but its first
component, which a repair edits (the evaluator's `ref.lost`). An assembly
reads its components and joints. An assembly containing itself, at any
depth, is a `dag.cycle`, refused as a command's result like any other.
The assembly commands touch these nodes, ranked after the features
(components, then joints, then assemblies, each by id).

- **Within a part**, a feature references only features before it in
  `history`. Reordering is a command that is refused when it would break
  that.
- **Across parts**, a feature may reference another part's feature output
  (a sketch projected from another part's face, a body used as a tool).
  Such an edge makes the whole referenced prefix of the other part an
  input. (As built, a feature naming another part's body is still
  `input.cross-part`; a joint's anchor is the one cross-part reference.)
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
6. Once the part is evaluated, mesh its *final* bodies (each slot's last
   version that no later feature consumed) and publish a `Meshed` event
   per body, coarse then refined. Meshes are cache like any result: not
   persisted, so no schema bump, and a cached version keeps its meshes.

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
version (none for a built-in), the SI value of each
parameter the record sets (not its text: `1 cm` and `10 mm` hash alike; a
default is the type's, which `type_version` covers), the choices and
each input as the geometry it resolved to (a plane as its frame), by name,
so the hash is computable from the record alone, without the type's form
(a frozen feature's staleness needs that), and a
sketch as its solve reads it (stored positions, resolved dimensions); the
feature's name is not an input. It is canonical JSON through BLAKE3,
written as 64 hex digits. A failure's codes: `input.unavailable` (a read
feature or parameter has no value; its `refs` name it), `ref.lost` (with
candidates), `input.wrong-kind`, `feature.output` (a type filled its
slots other than it declared), `expr.*`, `kernel.*`. A kernel failure
keeps its `KernelCall` in the event. Each feature's outcome is an
`EvalEvent { feature, outcome }`, the outcome tagged by `status` (`ok`
with its hash, whether it was cached and its slots as plain data;
`failed`; `suppressed`; `rolled-back`), and crosses as JSON.

**The assembly pass** (ADR-0029 §3 to §5 and §11; as built,
`Evaluation::assemblies`, an `AssemblyOutcome` per assembly). Once every
feature has its state, each assembly is solved in the DAG's order, so a
nested one before the assemblies that hold it. An anchor resolves against
its part's *final bodies* (those a mesh is made of, §Bodies across
features): its path must lead to a part (else `ref.lost`), its axis name
must be a face or an edge and its origin a face (else `input.wrong-kind`),
and exactly one final body must hold both names, else `ref.lost` with the
part's nearest names of that kind as candidates; a part with no final body
is `input.unavailable`. Its frame is a recorded `anchor_frame` call, cached
by a key over the Arris version, the input hash of the feature that made
the body's last version, the slot and the names, so a joint follows an
upstream edit and no other change calls the kernel. The assembly's input
hash covers each component's pose values in SI and nested assembly's hash,
and each joint's kind, flip, values in SI and anchors (path and frame key,
or the diagnostic); a hit is the stored outcome, `cached`. Joints that are
loops by structure (anchors on one component, a child with a pose, a child
an earlier joint by id places) are `assembly.loop`; forward kinematics
then places the grounded components, each component with neither pose nor
live joint at the identity (`assembly.unplaced`, a warning, also when its
joint failed), and each joint's child once its parent is placed; joints
left waiting on each other close a loop, and the last by id is
`assembly.loop`, its child unplaced. A value outside its limits is
`assembly.limit`, a warning, and applied. The outcome lists every
component at every depth by its path from the assembly, with its pose as a
`Frame` in the assembly's frame, and each joint `ok` (its value plus
offset, both anchors' frames, any warning) or `failed` with a diagnostic.

Forward kinematics is `arrix_doc::place`, a pure function of an assembly,
its nested outcomes' frames and its values, with one value optionally
overridden. The evaluator's pass and the app's joint drag both call it, so a
drag's preview is the pose the release's evaluation gives (ADR-0030 §3); it
reads no document and makes no kernel call.

**The mass pass** (ADR-0031 §4; as built, `Evaluation::masses`, `Masses`).
After the assemblies, each final body gets a `BodyMass`: its material (the
part's per-body override, else the part's) gives a density, a quantity of
kind density evaluated like any expression, and the kernel's unit-density
volume, centroid and inertia (about the centroid, held per body version so
a density edit makes no kernel call) scale by it into an `Inertial` in the
part's frame. A `MassOutcome` per part sums its bodies; one per assembly
sums every placed part at its solved pose (the inertia rotated, the
centroids weighted, the inertias moved to the common centre by the
parallel-axis theorem, `arrix_doc::Inertial`, pure). A body without a
material or with a material without a density is `mass.no-material`, one
whose density does not evaluate to a positive number `mass.density`, a
component `assembly.unplaced` is left out as `mass.unplaced`; each is a
warning, the body's volume and centroid stay, and the totals above it are
`partial`, a lower bound. Never a default density.

**The snapshot an exporter reads** (ADR-0037; as built, `Evaluator::export_with`).
After a finished evaluation, an exporter sees the document through the
plugin API's `Snapshot`, built from the `Evaluation` and the evaluator and
never stored: the parts with their final bodies and materials, each body's
refined mesh (the cached `MeshDetail::Refined`, positions made absolute),
each part's `MassOutcome` as an `Inertial` or its diagnostic, and the
assemblies, whose components come at every depth with their solved poses and
whose joints carry their limits (the expressions evaluated) and, for a joint
that solved, both anchors in their own component's frame, which does not
move with the joint's value. The host evaluates the exporter's options
(`export_with`'s `params` and `choices`) as a feature's, refuses a subject
of the wrong kind (`export.subject`) and a file name that is not relative
(`export.path`), and calls no kernel but for a mesh. Nothing persists, so
the schema does not change.

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
    Imported { feature: FeatureId, entity: u64, instance: u32 }, // a STEP file's entity (ADR-0029)
    // Primitive (C2) joins with primitives.
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
root  := ('sweep.' | 'revolve.') ID ('.' N)? '.' part | 'plugin.' ID '.' ID | 'frozen.' ID '.' N
      | 'import.' ID '.' N64 ('.' N)?
part  := 'start-cap' | 'end-cap' | curve-part '.' ID
step  := 'mod.' ID ('.' N)? '.' N | 'inst.' ID '.' N | 'gen.' ID ('.' N)? '[' name (',' name)* ']'
```

`ID` is an id's 13 characters, `N` a decimal `u32` and `N64` a `u64`, both without leading zeros,
`curve-part` one of `side`, `start-edge`, `end-edge`, `rise`,
`start-vertex`, `end-vertex`, `cavity`: `face:sweep.<feature>.side.<curve>`.
The optional `N` is the sweep's index among those one feature's evaluation
made, from 0 in call order; 0 is not written, and a written 0 is an error
(ADR-0011), so a feature that sweeps twice names its caps apart
(`end-cap`, `1.end-cap`). A `revolve` root has the same shape as a
`sweep` one (ADR-0010); an `inst.<feature>.<N>` step names an entity of the
`N`-th copy a pattern or mirror made, `N` from 1, so changing a pattern's
count renames no copy that stays. The optional `N` after a `mod`'s or a
`gen`'s id is the place of the call among the booleans (and blends) the
feature made, from 0 in call order, not written when 0 and an error when
written (ADR-0014): `mod.<F>.<k>` is boolean 0's piece `k`, `mod.<F>.<n>.<k>`
boolean `n`'s, so one feature may cut twice. A parse error names the byte
it stopped at; `gen` nests at most 32 deep.

An `import.<F>.<entity>[.<instance>]` root names an entity of the file an
import feature read: `entity` is the file's own entity number (a `u64`,
Arris's `Role::File`), `instance` which of the instances of that entity the
reader made, from 0; 0 is not written and a written 0 is an error, as for a
sweep's index (ADR-0029). It is stable for one file; a vendor's next
revision can lose a name, which is `ref.lost`, never a re-bind. `instance`
is the placement of the solid read (Arris's `FileEntity::instance`), and
every entity of one read body has the solid's. The `entity` is the face's
`ADVANCED_FACE`, the edge's `EDGE_CURVE`, the vertex's `VERTEX_POINT`; an
edge the file left out and the reader rebuilt (a seam, a degenerate edge)
takes its face's. Where the reader made several entities of one kind from
one file entity (an edge split where it runs through a pole), the first in
the reader's order keeps the name and piece `k` adds `mod.<F>.<k>`, as a
boolean's pieces do (`crates/arrix-kernel/src/import.rs`).

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
- **Through a boolean** (`fuse`, `cut`, `common`, as built; ADR-0010,
  ADR-0014): piece `0`
  of an operand's entity keeps its name, so a face trimmed or holed but
  not split is named as before; a piece `k > 0` is the name with
  `mod.<feature>.<k>` (`mod.<feature>.<n>.<k>` for the feature's `n`-th
  boolean, `n > 0`), `k` its place in Arris's split order, so `k` is
  the same after an edit that keeps which entities bound each piece
  (`crates/arrix-kernel/tests/booleans.rs`). A reference to one piece of
  a split face follows Arris's order (its ADR-0009, input ids), which a
  redraw of the curves bounding the pieces can change: the bare name
  then moves to the other piece
  (`probe_m2.rs`, `piece_0_of_a_split_follows_the_profile_s_walk`). Where a piece comes from both
  operands (two faces flush), the target's origin names it. An entity the
  boolean made is `gen.<feature>[…]` of its origins' names (`gen.<feature>.<n>[…]`
  for boolean `n > 0`), sorted, after
  the root and chain of the first of them, since every name has a root:
  a pocket's wall is `face:sweep.<tool>.side.<k>/gen.<cut>[face:sweep.
  <tool>.side.<k>]`, the edge where it meets the plate's bottom `edge:
  sweep.<plate>.start-cap/gen.<cut>[face:sweep.<plate>.start-cap,face:
  sweep.<tool>.side.<k>]`. An entity with no nameable origin, or two with
  one name, fails the call as `kernel.naming`.
- **Through a blend** (`fillet`, `chamfer`, as built): the faces an edge's
  blend trims and the edges it shortens split nothing, so they keep their
  names (piece `0`). What the blend made is `gen.<feature>[…]` of the
  edges it came from, sorted: a blend face of one edge is `…/gen.<F>[<edge>]`,
  a miter's shared edge `gen.<F>[<edge>,<edge>]`, a corner's patch face
  `gen.<F>[<edge>,<edge>,<edge>]`. Entities made from the same edges (a
  blend's two contact edges, its end vertices) would share that name, so
  each also lists the names of what it touches: an edge its faces, a
  vertex its edges, which the operation did not make from the same
  origins. Naming one edge of a tangent chain blends the chain, and every
  chain edge's blend face is `gen.<F>[<its own edge>]`, so the same names
  come out whichever edge was named. A name nests one `gen` per blend of
  a body that was blended again, within the limit of 32 (`crates/arrix-kernel/tests/blends.rs`).
- **Through a copy or a mirror** (`Kernel::copy`, `Kernel::mirror`, as
  built): Arris modifies every entity one-to-one, so each entity is its
  original's name with `inst.<feature>.<i>`, `i` from 1 (a mirror is
  instance 1); a copy of an instance adds its own step. A pattern applies
  its copies one by one, each its own boolean (ADR-0014): a hole's wall in
  copy `i` is `…/inst.<F>.<i>/gen.<F>.<i-1>[<wall>/inst.<F>.<i>]`, so
  raising or lowering the count renames no surviving hole
  (`crates/arrix-kernel/tests/instances.rs`).
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
    slots: BTreeMap<SlotName, BlobRef>, // Arris body bytes (`.arrisbody`), one blob per body
    names: BlobRef,                     // `.json`: slot name to its body's name table, each name to the entity's index in the written body
}
```

It is written whenever the feature evaluates successfully with its plugin
present, so the file always carries the latest result. It is evaluation
output, not a command (ADR-0009): the session keeps the latest live result
of each plugin feature with its input hash, beside the document, and
`save` writes it into the feature's record and blobs when that hash is the
feature's current one, and otherwise keeps what the document was opened
with. It has no undo entry and no generation and is never stale-rejected;
evaluation being deterministic, undo, re-evaluate and save gives the
bytes of the earlier save. The bytes are canonical (a body copied into a
fresh model, written with an empty record), so another evaluation of the
same feature writes the same blobs.

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

**As built** (`arrix_doc::eval`, `frozen.rs`). A feature is frozen when
its type is a plugin's and the registry lacks it (*missing*), its record is
a newer `type_version` than the plugin has, or the plugin's version is not
caret-compatible with the one that stored the result (both *version*). A
core type the registry lacks is `feature.unknown-type`, not frozen. With no
stored result a frozen feature fails as `plugin.frozen.missing` or
`.version`, with no outputs. Otherwise its inputs are resolved by the shape
of their references alone (a body slot is a body, another slot or a face a
plane, a region a region) and hashed under the stored plugin version; a
hash unequal to the stored one is *stale*. Its outputs are the stored
bodies, loaded from their blobs (`blob.missing`, `blob.corrupt`) and named by
the table, under the **stored** hash, so what reads them hashes as it did
live. The event carries `frozen: {reason, stale}` and an informational
diagnostic with the reason; `plugin.frozen.stale` is a warning on a stale
feature and on every feature that reads one, directly or through the DAG.
A slot kind the stored result lacks (a plane slot) is not restored; only
body slots are stored. Without its type a frozen feature's body slots are
those of its stored result, which is what a name into it is looked up in;
it can modify no body input, the type's `modifies` being unknown.
`arrix eval --without-plugin <id>` evaluates with the plugin's types
left out of the registry.

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
    AddFile { bytes: Vec<u8> },                           // named by its BLAKE3; base64 in JSON
    RemoveFile { blob: BlobRef },                         // AddFile's inverse
    AddAssembly { assembly: Assembly },                   // with its components and joints
    DeleteAssembly { assembly: AssemblyId },
    AddComponent { assembly: AssemblyId, component: Component },
    EditComponent { assembly: AssemblyId, component: Component },  // replaces the same id
    DeleteComponent { assembly: AssemblyId, component: ComponentId },
    AddJoint { assembly: AssemblyId, joint: Joint },
    EditJoint { assembly: AssemblyId, joint: Joint },     // replaces the same id
    DeleteJoint { assembly: AssemblyId, joint: JointId },
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
    inputs: BTreeMap<String, Option<Input>>,  // a list input is set whole
}
```

The file and assembly commands (ADR-0029 §9): `AddFile` holds the bytes as
the blob `<hash>.step` (STEP is the only imported kind so far) and adding
bytes the document holds changes nothing; `RemoveFile` is refused while a
record names the blob. An import is one gesture, a `Group` of `AddFile`
and `AddFeature` (a record naming a file the document lacks is malformed,
`doc.invalid`). A component of a deleted part or assembly, or a joint
through a deleted component, is refused as `assembly.dangling` when the
command or group ends, never cascaded, so a group may delete the joint,
the component and the part in order. Component and joint ids are unique
across the document. They touch the nodes `Assembly`, `Component` and
`Joint`, which the DAG gains edges for later in M1.

A plugin feature's frozen result is not a command: it is evaluation
output the session writes on save (§Frozen results, ADR-0009), with no
generation and no undo entry.

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
- **A registry change is no command.** Loading or reloading a plugin
  (`Request::LoadPlugin`, `ReloadPlugin`, ADR-0024) makes a generation,
  announced by `SessionEvent::Registry`, with no `Change`, no undo entry
  and no stamp; the document does not change and a registry change is
  never persisted.
- **Plugin commands** are composed from core commands and the plugin's own
  `PluginData` edits (`docs/PLUGINS.md` §Commands), below.

**Plugin commands** (ADR-0004) run on the client that issues them: the
plugin expands a gesture into one `Group` of core commands and
`PluginData` edits, which is what is submitted, and its undo entry is
that group's inverse, computed by the core. A `PluginData` edit is a
whole-record put or remove, so the core inverts it without the plugin.
A plugin never supplies an inverse, and the authority never runs plugin
code, so undo works with the plugin absent.

As built (ADR-0038), expansion is a request of
its own: `Request::Expand { command, context, params, choices, inputs,
ids, base }` runs the plugin where the session's registry is, once `base`
is evaluated (an older base is stale), and answers `Expanded { group }`
with no generation and no undo entry. The client then submits that group
as an ordinary envelope on the same base, so a commit in between makes the
submit stale, and a form's preview is the group's `Preview` (ADR-0019).
`ids` are fresh ids the client mints (§Identifiers), the spec's `max-new`
of them; a record naming an id outside them, an expression that does not
parse or a feature type the registry lacks makes the group
`command.invalid`, refused whole. The group's records are core commands
only until `PluginData` lands.

## Sketches

A sketch is the record of a `core.sketch` feature; its model and solver
are `arrix-sketch` (`docs/UI-RENDERING.md` §Sketch mode for the
interaction).

- **Entities**: points, lines, arcs and circles, each with an id and a
  construction flag. Points, curves and constraints share one id space,
  `SketchEntityId` (§Identifiers), minted by the command's author, never
  by `arrix-sketch`; lengths compare to `arrix_core::LENGTH_TOLERANCE`
  (1 µm). Ellipses, elliptic arcs and B-splines are ported
  behind a feature gate that stays off until a scenario reaches them
  (`SEED.md` §8.2 rule 7). Projected geometry (an
  edge or vertex of a body projected onto the plane) is an entity with a
  `Ref::Topo` source, re-projected on evaluation and failing like any lost
  reference. As stored (schema 3): `sources`, a map from the entity's id
  to the edge or vertex name, on a point, line, arc or circle only (the
  `conics` gate stays off); `SketchEdit` puts or, with `null`, removes
  one, and removing the entity sweeps its source. The evaluator reads a
  source as a DAG edge, and before the solve and the input hash read the
  sketch it projects each source onto the sketch's plane (one recorded
  `project` call per entity, the body found as for any `Ref::Topo`) and
  moves the entity there: a line keeps whichever end is nearer each
  projected end, so shared points do not swap. The solver holds a
  projected entity's points and a circle's radius as it holds a fixed
  point (not variables; `Fixed` in the diagnostics), so a constraint to
  it drives the drawn geometry. Its failures name the entity
  (`Ref::Sketch`) and the source: a lost source is `ref.lost` with
  candidates, a source seen end-on or as an ellipse the kernel's
  `kernel.degenerate.projection-collapses` or
  `kernel.unsupported.projection` (record kept), and a projection of
  another shape than the entity's `sketch.source-kind`.
- **Constraints and dimensions** reference entities by id. A dimension is
  a constraint with a value, driving or reference, and its value is an
  expression, so it can name a parameter. The kinds a document can hold
  are the kinds the UI can create (`docs/UI-RENDERING.md` §Sketch mode
  lists them); the solver knows more, behind the `conics`, `snells-law`
  and `unreached` gates (`unreached` is the kinds sketch mode has no tool
  for), and a document holding a gated kind does not deserialise.
- **The last solved positions are recipe.** They are the solver's starting
  point, and with a nonlinear solver the starting point selects which of
  several solutions it lands on. Stored positions make re-solving
  deterministic and keep a sketch on its branch after an edit.
- **Profiles** are the faces of the planar arrangement of the
  non-construction curves. A feature references a region by a key
  (`Ref::Region`, ADR-0006): the ids of every entity that bounds it, its
  holes' included, plus an interior sample point in whole nanometres, as
  far inside the face as it allows. A key that resolves to no region, or
  to two, is a lost reference, never the nearest region: `ref.lost`, the
  sketch's current regions as candidates, most shared bounding entities
  first. A hole drawn inside a region later makes it another region, and
  the feature holding its key is re-picked. A `region` input resolves to
  the region's `Profile` on the sketch's plane. Arris ask A5 (`region2` as public API) would make the
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
assemblies/<assembly-id>.json   one assembly: its components and joints
materials/<material-id>.json    one material: its density and appearance
plugins/<plugin-id>/   one plugin's section: its own schema version and its records
blobs/<blake3>.<ext>   frozen results (.arrisbody), name tables, imported files, thumbnails
```

- **Deterministic JSON.** Keys sorted, two-space indent, `\n` line ends, a
  trailing newline, floats in shortest round-trip form. Saving the same
  document twice gives identical bytes; so does saving, loading and saving
  again. A test holds both.
- **Deterministic zip.** Entries in sorted path order, a fixed timestamp
  (1980-01-01), no extra fields and no directory entries, JSON deflated and
  blobs stored. The same document gives the same `.arrx`. As built
  (`arrix_doc::to_zip`, `save_zip`; `from_zip`, `open_zip`): the reader
  refuses the whole zip for a path outside the tree (not `document.json`,
  `params.json` or a file under `parts/`, `assemblies/`, `materials/`, `plugins/` or `blobs/`, or with
  an empty, `.`, `..` or backslash component), a path twice, a method
  other than stored and deflate, or a size or CRC-32 that fails; a
  directory entry of an ordinary zip tool is skipped. No zip64.
- **Blobs are content-addressed**: the name is the BLAKE3 of the bytes, so
  identical results are stored once and a changed result is a new file.
  Blobs no record points at are dropped on save.
- **Versioning.** `document.json` carries `schema` (an integer, 4 since C2 M1).
  Opening an older schema runs forward migrations in order, each a pure
  function over the JSON tree with its own fixture document. A newer
  schema than the build knows is refused with a message naming both. A
  feature record's `type_version` is migrated by its feature type (a
  plugin's own migration, or kept frozen if the plugin is absent).
- **What opens and saves today** (`arrix_doc::open`, `arrix_doc::save`):
  the directory form, as `document.json` (`schema` 4; nothing else in the
  header is interpreted yet), `params.json` (the parameters by id; absent
  reads as none), `parts/<part-id>.json` (one part, its id matching the
  file's name) and `blobs/<blake3>.<ext>`. `arrix_doc::to_json` is the
  writer: it goes through `serde_json::Value`, whose map is ordered, for
  sorted keys. A record with a field this build does not know is refused,
  never dropped on the next save. The opened document is checked as a
  command's result is (`Document::validate`). A newer schema is refused
  with a message naming both (`the document's schema is 5, newer than this
  build's 4`), a schema below 1 as one no ArriX wrote. `save` returns the
  files by path; the caller writes them and removes what an earlier save
  left.
  Every command undone restores the earlier save's bytes, and redone the
  later one's (`crates/arrix-doc/src/save/tests.rs`).
  `tests/docs/empty/` is the empty document; `tests/docs/gear-on-plane/`
  is M1's first scenario, written by the commands of
  `crates/arrix-cli/tests/gear_on_plane.rs` and checked against them on
  every run (`UPDATE_SNAPSHOTS=1` rewrites it).
- **Schema 2 and the migration chain.** Schema 2 adds
  `FeatureRecord.frozen` (omitted when absent) and `blobs/`. `open` parses
  the JSON files into a tree, runs the links of `arrix_doc`'s `migrate`
  chain from the document's schema up (each a pure function over the tree,
  `doc.migration` naming the link that failed), and only then reads
  records. 1 → 2 is the identity bar the header; its fixture is
  `tests/docs/migrations/schema-1/`, opened and re-saved by
  `crates/arrix-cli/tests/migration.rs` to the bytes of
  `tests/docs/gear-on-plane/`.
- **Schema 3** (ADR-0012) adds two stored shapes: a feature's input may be
  an array of references (`Input`, a `Ref` object or an array of them,
  read untagged; an empty array is the input unset), and a sketch may
  hold `sources`, a projected entity's edge or vertex name by the
  entity's id (§Sketches). Neither shape is in a schema-2 document, so 2 →
  3 is the identity bar the header; its fixture is
  `tests/docs/migrations/schema-2/`, a copy of `tests/docs/slice/`
  re-saved to the slice's bytes but the header, checked by the same test.
  An older build refuses a schema-3 document by the schema rule, never by
  failing to parse an array. A blob is read only when a frozen result
  points at it: a missing one is `blob.missing`, one whose bytes do not
  hash to its name `blob.corrupt`, and one nothing points at is dropped on
  the next save.
- **Schema 4** (ADR-0029) adds `FeatureRecord.file` (omitted when absent;
  its blob is read and kept like a frozen result's, `blob.missing` and
  `blob.corrupt` alike) and `assemblies/<assembly-id>.json`, the id
  matching the file's name. Neither is in a schema-3 document, so 3 → 4 is
  the identity bar the header; its fixture is
  `tests/docs/migrations/schema-3/`, a copy of `tests/docs/slice/` as it
  was saved at schema 3, re-saved to the slice's bytes by the same test.
- **Schema 5** (ADR-0031) adds `materials/<material-id>.json`, the id
  matching the file's name, and a part's `material` and `body_materials`
  (each omitted when empty). None is in a schema-4 document, so 4 → 5 is
  the identity bar the header; its fixture is
  `tests/docs/migrations/schema-4/`, a copy of `tests/docs/assembly/` as it
  was saved at schema 4, re-saved to the assembly's bytes by the same test.
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
