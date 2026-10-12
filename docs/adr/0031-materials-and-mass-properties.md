# ADR-0031: Materials and mass properties

- Status: Accepted
- Date: 2026-10-12

## Context

C2's accept (ADR-0027) needs mass, centroid and inertia per body, part and
assembly, and appearance colours that survive STEP (ADR-0032). The document
holds neither a density nor a colour. `arrix-plugin-api`'s `MassProperties`
is volume, area and centroid; Arris 0.6's `mass_properties` also returns an
inertia about the centroid at unit density. Mass is a fact about a design,
so it belongs to the recipe's inputs (a material), not to the cache.

## Decision

1. **Materials are document data.** `Document.materials:
   BTreeMap<MaterialId, Material>`; `Material { id, name, density: Expr,
   appearance: Rgb }`. The density is an expression of the new quantity
   kind `Density` (`kg/m3`, `g/cm3`; SI inside, `7.85 g/cm3` is 7850), so a
   parameter may drive it. Names are unique within the document.
2. **Assignment is per part, with a per-body override.** `Part.material:
   Option<MaterialId>` and `Part.body_materials: BTreeMap<SlotRef,
   MaterialId>` for a final body that differs from its part. Both are
   omitted from the file when empty. Decided by the human, 2026-10-12.
3. **Commands**: `AddMaterial`, `EditMaterial`, `DeleteMaterial` (refused
   while assigned, `material.in-use`), and `AssignMaterial { part, body:
   Option<SlotRef>, material: Option<MaterialId> }`, each with its inverse
   (ADR-0003 rules). The dependency DAG gains an edge from a part to the
   materials it uses, so a density edit re-evaluates mass only.
4. **Mass is evaluated, not stored.** A body's `MassOutcome` is its
   material's density times Arris's unit-density volume, centroid and
   inertia (about the centroid). A part sums its final bodies; an assembly
   sums its components at their solved world poses, inertia moved by the
   parallel-axis theorem. This is pure math in `arrix-doc`; the kernel
   supplies unit-density properties only. Results are cached by input hash
   like any other.
5. **No material is not a default.** A body with no material is
   `mass.no-material`: its volume and centroid are kept, its mass is not
   summed, and the part and assembly totals are flagged partial. Never a
   silent default density.
6. **No library in M3.** Materials live in the document only. Presets
   (steel, aluminium, ABS, PLA) as a user config file (ADR-0015) are a
   backlog line. Decided by the human, 2026-10-12.
7. **`arrix-plugin-api` is unchanged.** Its `MassProperties` stays volume,
   area and centroid; C3's robotics plugin asks for density and inertia by
   an API change when it needs them. The eval line and the protocol gain
   `materials` and `mass` additively (ADR-0026).
8. **Schema 5** carries `materials/<id>.json` and the part's two fields;
   4 to 5 is the identity but for the header.

## Consequences

- Mass, centroid and inertia are available headless, on the eval line and
  the protocol, for every level of the tree.
- A model with no materials evaluates exactly as before; its mass is
  flagged, not wrong.
- A density is an expression, so a sweep over it works with no new code.
- A body's material follows its slot, so a rename of the part's feature
  order does not lose it; a slot that vanishes drops its override with the
  slot's other references (no silent re-bind).

## Alternatives considered

- **Per body only**: no part-level default, so every final body of every
  part is assigned one by one; the common case (one material per part)
  costs the most.
- **A density stored on the feature**: mixes geometry history with a
  property of the finished part.
- **A default density when none is set**: a silently wrong mass.
- **Mass in the plugin API now**: widens a semver-sensitive surface for a
  consumer that does not exist yet.
- **A shipped material library**: scope beyond the cycle's in-list, and a
  config format to design for four entries.
