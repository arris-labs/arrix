# ADR-0029: Assemblies live in the document

- Status: Accepted
- Date: 2026-10-11

## Context

`docs/DATA-MODEL.md` §The document reserves an `assemblies` map, and the
DAG admits cross-part references, but a body of another part is
`input.cross-part` "until assemblies say what a version across parts is".
C2 M1 (ADR-0027) must say it, and must say how a joint finds its anchors
by persistent name, how forward kinematics (FK) places a component, and
what a document holding a loop or an unplaced component does. The
predecessor prototype's model (components grounded by a pose unless a joint
places them; fixed, revolute and prismatic joints; anchors as an axis plus
an optional origin face; expression-driven values) is ported and
generalised by nesting and by references by persistent name.

## Decision

1. **Model** (`arrix-doc`). `Document.assemblies: BTreeMap<AssemblyId,
   Assembly>`; `Assembly { id, name, components, joints }`; `Component {
   id, name, of: Part(PartId) | Assembly(AssemblyId), pose: Option<Pose> }`,
   a pose grounding it; `Joint { id, name, kind: Fixed | Revolute |
   Prismatic, parent: Anchor, child: Anchor, value: Expr, offset: Expr,
   limits: Option<[Expr; 2]>, flip: bool }`; `Anchor { path:
   Vec<ComponentId>, axis: PersistentName, origin: Option<PersistentName>
   }`, the path from the joint's assembly down to a part. A `Pose` is a
   position (three lengths) and URDF's fixed-axis roll, pitch, yaw (three
   angles), all expressions. New ids `AssemblyId`, `ComponentId`, `JointId`.
2. **A version across parts is the part's final state**, as its own
   evaluation leaves it, rollback honoured: an anchor's input hash is the
   hash of the last evaluated feature of each slot it names. This closes
   the "until assemblies say" for anchors only; features keep
   `input.cross-part`, and there is no in-context design in C2.
3. **An anchor's frame.** The axis entity gives z and a point: a
   cylindrical face's axis, a circular edge's centre and normal, a
   straight edge's line, a planar face's normal at its frame origin. An
   origin face moves the point to where the axis meets its plane. x is the
   origin face's u projected, else the world axis least aligned with z (X
   before Y before Z) projected, so the frame is deterministic. `flip`
   turns the child's frame half a turn about its x.
4. **FK.** Grounded components sit at their pose. A joint places its child
   at `parent_world · F_parent · motion(value + offset) · F_child⁻¹`, the
   motion a rotation about z (revolute), a translation along z (prismatic)
   or none (fixed). The structure is a forest: a component with a pose and
   a joint, or with two joints, is `assembly.loop` on the later joint (by
   id), which is then ignored; a component with neither is
   `assembly.unplaced`, placed at identity. A value outside its limits is
   `assembly.limit`, applied as given. Nested assemblies compose their
   solved poses. The math is pure and lives in `arrix-doc`; the frames are
   recorded kernel calls (`anchor_frame`). Fail soft: the document can hold
   these states mid-edit, so they are diagnostics, not refusals.
5. **A lost anchor** is `ref.lost` on the joint with candidates, never a
   silent re-bind (`SEED.md` §8.2); the child it would have placed is
   `assembly.unplaced`; every other result stands.
6. **Imported names.** `NameRoot::Imported { feature, entity: u64,
   instance: u32 }`, from Arris's `Role::File(FileEntity)`; text
   `import.<ID>.<entity>[.<instance>]`, instance 0 not written and an
   error when written, as ADR-0011's index. Stable for one file; a
   vendor's next revision can lose a name, which is `ref.lost`.
7. **`core.import-step`**: no inputs, a `solid` count parameter (from 1 in
   the reader's order, default 1), one output slot `body`; the file is the
   record's `file`. A refused solid is `kernel.import.<refusal-kind>`
   naming the file entity; no file is `import.no-file`; a `solid` past the
   file's count is `import.no-solid`. Built in, so never frozen. One
   feature reads one solid; a multi-solid or structured file is several
   features, then M3's product-tree import.
8. **Schema 4.** `FeatureRecord.file: Option<ImportedFile { name, blob }>`
   (a base name for display, never a path) and `assemblies/<id>.json`; the
   zip reader admits `assemblies/`. A blob a record's `file` names is kept
   on save. 3 → 4 is the identity but for the header.
9. **Commands.** `AddFile { bytes }` (named by its BLAKE3; inverse
   `RemoveFile`, refused while a record names it), `AddAssembly`/
   `DeleteAssembly`, `AddComponent`/`EditComponent`/`DeleteComponent`,
   `AddJoint`/`EditJoint`/`DeleteJoint`, each with its inverse. An import is
   one gesture, a `Group` of `AddFile`, `AddPart`, `AddFeature`. A
   component of a deleted part or a joint of a deleted component is refused
   (`assembly.dangling`), not cascaded.
10. **The DAG** gains assembly, component and joint nodes. A joint reads
    its anchors' parts (their final features) and its expressions'
    parameters; a component reads its part or assembly; an assembly
    containing itself, at any depth, is `dag.cycle`.
11. **Evaluation.** After the parts, each assembly in id order, nested ones
    first; an `AssemblyOutcome { components: world poses by path, joints:
    outcome each }` cached by its input hashes.
12. **Surfaces** (ADR-0026, additive): the eval line gains `assemblies`;
    the protocol gains the commands and a `SessionEvent::Assembled {
    generation, assembly, outcome }`.
13. **Deferred to M2's ADR**: whether a joint drag's preview solves FK
    client-side like a sketch drag, since no kernel call is involved.

## Consequences

- A name across parts is tested first, into imported and frozen geometry:
  the joint follows an upstream edit live and with the plugin removed.
- Loops and missing placements are visible diagnostics instead of a
  document that cannot be opened; a general mate solver stays a named cycle.
- Cross-part references are anchors only, so the DAG's new edges are few
  and checkable.
- Imported names are stable only for one file revision.

## Alternatives considered

- **Assemblies as separate `.arrx` files referencing each other.** Another
  identity and versioning problem; `SEED.md` §6.5 keeps parts and assemblies
  in one document.
- **Refusing a loop or an unplaced component as a command error.** A
  mid-edit document must be representable; fail soft with a diagnostic.
- **Anchors as raw geometry, not names.** Would not follow an upstream edit
  and would bypass the naming system the cycle exists to test.
- **A version across parts as the whole history.** A rolled-back feature
  would still move a joint; the final state is what the part is.
