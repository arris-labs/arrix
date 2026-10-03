# ADR-0010: The single-piece rule, the Instance step and the revolve root

- Status: Accepted
- Date: 2026-10-03

## Context

M2 names the result of every kernel operation its features need: revolve,
fillet, chamfer, patterns and mirror (`docs/ROADMAP.md` §M2,
`plans/c1-m2-naming`). The rule M1 built for a boolean
(`docs/DATA-MODEL.md` §Persistent naming) names an entity that a later
feature trimmed or holed `mod.<feature>.0`, even when it is the one piece
left. So a name changes whenever any feature touches the entity it names.

That breaks the scenario M2 is for. A hole is placed on a plate's top face;
a fillet is then added above it, removed, or reordered. The fillet trims the
top face, and every reference to the face, the hole's included, would take a
`mod` step the moment the fillet exists and lose it the moment the fillet is
gone. The same holds for any pattern or mirror, which touches whole bodies.

`mod.<feature>.<k>` exists to tell the pieces of a split entity apart. An
entity that is not split has no pieces to tell apart. Three words are also
missing: a copy of a body made by a pattern or mirror, and the root of a
revolve (two sweeps over one profile curve cannot share the sweep root, which
the extrude's `Side(CurveKey)` already uses for the feature that made it).

## Decision

1. **Piece `0` keeps the name.** An entity the operation modifies keeps the
   origin's name when it is piece `0` of its origin's split order, whether
   the origin was trimmed, holed or left whole. Only pieces `k > 0` take
   `mod.<feature>.<k>`. This supersedes the `mod` paragraph of
   `docs/DATA-MODEL.md` §Persistent naming ("a face holed but not split is
   `mod.<feature>.0`"); it supersedes no ADR.
2. **An Instance step.** `NameStep::Instance { feature, index }`, text
   `inst.<ID>.<N>`, names an entity of the `index`-th copy a pattern or mirror
   made, `index` from 1. Raising or lowering a pattern's count renames none
   of the copies that stay. The DAG reads the feature an `Instance` step
   names, as it does for `mod` and `gen`.
3. **A revolve root.** `NameRoot::Revolve { feature, part }`, text
   `revolve.<ID>.<part>`, shares `SweepPartName` with the sweep root, so a
   revolved profile's side face is rooted at the key of the curve that
   swept it, as an extruded one is.
4. **Stored form.** The text grammar gains two words, additive. A name stored
   as `…/mod.<F>.0` by M1 no longer exists; C1 is unreleased, so the
   schema does not change and M1's scenario documents and goldens are
   regenerated, their diff read (`plans/c1-m2-naming`, open question 1).

If Arris's split order for piece `0` turns out to flip under a plausible edit,
this decision is reopened by the probe of step 2 and the human decides
between it and a forward-closure resolver.

## Consequences

- A reference to a face survives a feature that trims it, and survives that
  feature being added, removed or reordered above it.
- A name no longer says whether its entity was touched. A diagnostic that
  wants to say so reads the chain's feature ids, not a `mod` step.
- The lost-reference candidates rank by chain prefix as before; a name
  with fewer steps ranks nearer.
- Everything M1 names through `fuse` and `cut` changes text once.
- A feature that sweeps twice and names both caps alike is not helped
  by this; the `index` it would need in the sweep root stays a backlog line
  until a boolean needs it.

## Alternatives considered

- **Keep `mod.<F>.0`**: names change with every neighbour feature, which is
  the failure the scenario describes.
- **A forward-closure resolver** that follows a name through later features
  instead of fixing the text: costs a resolver pass on every read and makes a
  name's meaning depend on the whole history; kept as a later ADR if this
  rule proves too loose.
- **A schema bump with a migration** rewriting `…/mod.<F>.0`: nothing to
  migrate while C1 is unreleased.
