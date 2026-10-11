# ADR-0027: C2 is assemblies and exchange

- Status: Accepted
- Date: 2026-10-11

## Context

`SEED.md` §7 gave C2 eight capabilities: assemblies, materials, mass
properties, measure, section view, STEP import with assembly structure
(Arris ask A4), 3MF and DXF/SVG sketch exchange. That is about the size of
C1 with no stated scariest unknown and no milestone order. Three of the
eight wait on Arris's query cycle (A6), which is half released: distance
and ray fire landed unreleased, interference and the planar section have
not. Two of them (3MF, DXF/SVG) retire no unknown, and DXF/SVG import means
splines or bulges in the sketcher, a scope question the cycle does not
need (`SEED.md` §8.2 rule 7).

The north-star demo (`SEED.md` §7) needs a vendor STEP imported and mated,
and a robotics plugin that reads the assembly. It does not need measure,
section view, 3MF or DXF/SVG. The one real unknown is a persistent name
across parts and instances, into imported and frozen geometry.

## Decision

C2 is "assemblies and exchange": the exchange unit (ADR-0028), `core.import-
step`, assemblies in the document (ADR-0029), materials and mass
properties, and STEP with product structure both ways (A4). Three
milestones, risk first:

1. **M1, assemblies in the document, headless**: import, the model, the
   commands, FK in the evaluator, `.arrx` schema 4, `arrix eval` and the
   protocol.
2. **M2, the app**: instanced meshes, picking by component path, the
   assembly tree, the joint form and tool, the drag.
3. **M3, exchange and the accept**: STEP with structure both ways,
   materials and mass, the cycle's accept against Open CASCADE.

Measure, section view, 3MF and DXF/SVG move to a named cycle, "the
mechanical core completed", scheduled once Arris releases its query cycle.
`SEED.md` §7's C2 line and named list are amended, and `docs/ROADMAP.md`
§C2 is rewritten with the milestones. The product scenario this unlocks is
C3's: a vendor servo imported and mated to a bracket.

## Consequences

- Each milestone retires a named unknown, and nothing in C2 waits on a part
  of Arris that is not written.
- C3 is reachable sooner, and joints and mass properties are what robotics
  reads.
- The mechanical core is unfinished at C2's close: no measure tool for a
  while. If Arris releases the planar section before M2, measure and
  section can rejoin C2 as an M4 at little cost.

## Alternatives considered

- **C2 as written, one cycle.** Stalls at a gate Arris owns, and spends
  breadth (3MF, DXF/SVG) before the unknown is retired.
- **Exchange first, assemblies after.** Wrong risk order: the scary part is
  the document change, exchange mostly wraps Arris, and the accept needs
  assemblies.
