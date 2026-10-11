# ADR-0030: Assemblies in the app

- Status: Accepted
- Date: 2026-10-11

## Context

C2 M1 put assemblies in the document and solved them headless (ADR-0029).
M2 (ADR-0027) brings them to the app: instanced meshes, picking by
component path, the tree, the joint form and tool, and the drag. Today a
tab draws every part's final bodies at its origin, the scene holds one
model matrix per body, a pick resolves to a `PersistentName` alone, and
every form is a feature type's spec (ADR-0018). An assembly needs one body
drawn at several poses, a pick that says which component it hit (an
anchor is a path and a name), forms for records that are not features,
and a drag. ADR-0029 §13 left to this ADR whether the drag's preview
solves forward kinematics (FK) on the client, as a sketch drag solves on
the client (ADR-0005), since no kernel call is involved.

The predecessor prototype edited a part alone, at its own origin, and
placed components in a separate assembly context; that worked.

## Decision

1. **The context.** A tab looks at one `Context::Part(PartId)` or
   `Context::Assembly(AssemblyId)`, session state that issues no command.
   A document opens in its first root assembly (one no assembly contains,
   by id), else in its first part, so a document without assemblies looks
   as it did. Activating a part or an assembly in the tree switches.
   Features and sketches are edited in their part's context only, at the
   part's origin: a form, sketch mode or a feature's activation in the
   tree switches to its part first. A part's context draws its final
   bodies at its origin; an assembly's draws each placed component's at
   its world pose from the newest `Assembled` (an `assembly.unplaced` one
   at the identity, where FK put it). Decided by the human, 2026-10-11.
2. **The instanced scene** (`arrix-viewport`). A mesh is held per
   `BodyKey` and uploaded once; the scene draws a list of `Instance {
   path: Vec<ComponentId>, key, model }`, one draw each. A part's context
   is one instance per body with an empty path and the identity, so there
   is one draw path and one pick path. The model is the component's pose
   composed in f64 with the mesh's origin, cast to f32 last. The pick id's
   ten body bits index the visible instances (past 1,023 they draw
   unpickable, as bodies do today); a pick resolves to the instance's path
   and the entity's name. Hover, selection and a form's marks are a path
   and a name; locate, bounds, feature bounds and frozen badges are per
   instance.
3. **The drag is the client's** (decides ADR-0029 §13, as ADR-0005 decided
   a sketch's). A left drag on a component in an assembly's context moves
   the joint placing the deepest component of the picked path that a
   joint places; a grounded or unplaced one does not drag, and the status
   line says why. The UI thread reads the cursor against the joint's axis
   (ray and plane math) and composes FK from the newest `JointOutcome::Ok`
   (its `parent` and `child` frames) with a trial value, through
   `arrix-doc`'s pure FK, the same function the evaluator's assembly pass
   runs. The child's subtree moves by model matrices only; nothing is
   uploaded and nothing is asked of the authority until the release. The
   trial value is clamped to the joint's limits. A revolute drag whose
   plane of rotation is seen edge-on reads the angle on screen instead; a
   prismatic drag along an axis that faces the eye is refused with a hint.
4. **What a release commits**, one command for the gesture and none for a
   release where the drag started. A joint whose `value` is a literal gets
   `EditJoint` with the new literal, in display units. A joint whose value
   is one parameter's name gets that parameter's edit, so the drag never
   overwrites the expression; everything else reading the parameter moves
   with it. Any other expression refuses the drag with a hint to edit it
   in the form. Decided by the human, 2026-10-11.
5. **Forms the app builds.** A joint and a component have no feature spec,
   so the app builds their `FormView` from the record and the one renderer
   draws it (ADR-0018). A joint's: kind (a choice), the parent and child
   anchors (reference fields taking an axis and an optional origin face,
   each filled by a pick that carries its path), value, offset, the lower
   and upper limits, and flip (a choice of two, not a new field kind). A
   component's: name, what it instances (a choice of the parts and the
   assemblies that would not contain it), and its pose (three lengths and
   three angles; cleared means placed by a joint). An edit form commits one
   `EditJoint` or `EditComponent` per field; a create form is a draft until
   OK, one `AddJoint` or `AddComponent`. A lost anchor shows its
   candidates as a feature's lost reference does, and a pick replaces it;
   nothing re-binds it.
6. **Commands.** An Assembly ribbon tab, `core.assembly`, holds
   `core.import-step.create` (an open-file dialog behind a seam beside the
   save dialog, then one `Group` of `AddFile`, `AddPart`, `AddFeature`,
   ADR-0029 §9), `core.assembly.create` (`AddAssembly`, then its context),
   `core.assembly.component.create` and
   `core.assembly.joint.{fixed,revolute,prismatic}`; the last four apply
   only in an assembly's context. A keymap context `assembly` is active
   there, beside `part`.

## Consequences

- An assembly draws in one upload per body however many components
  instance it, and a part's context is the same code with one instance.
- A pick says which component it hit, which is what an anchor needs; the
  selection is session state, so nothing of it reaches the document.
- The drag answers within the frame without the worker, and the release's
  evaluation lands where the preview showed, since both run one function
  on the same frames. FK joins the sketch solver as the UI thread's
  sanctioned computation (`docs/ARCHITECTURE.md` §Threads); it is matrix
  products over frames the worker already published.
- A thin client can drag a joint with no kernel, as it can a sketch.
- Editing a part in place at a component's pose, a joint create form's
  preview (ADR-0019's preview carries no `Assembled`), and deleting
  components, joints and assemblies from the app wait for scenarios.
- A drag on a parameter-driven joint moves everything else that reads the
  parameter; that is what the value says.

## Alternatives considered

- **One view of everything**, every part at its origin with the
  assemblies beside them: no switching, but an imported part overlaps the
  part it mates to, and a pick could not tell a part's own body from a
  component's.
- **The drag as a `Request::Preview`** (ADR-0019) per frame: a round trip
  through the worker each frame for a result that needs no kernel, behind
  any commit being evaluated; and a thin client would wait on the network
  to see its own drag.
- **A drag on a parameter-driven joint refused**, asking for a literal:
  the scenario's joint is driven by a parameter, and the drag would be
  unusable on exactly the joints a user parametrised.
- **One scene body per component**, each uploading its part's mesh: N
  uploads of one mesh, and a refined mesh re-uploaded N times.
