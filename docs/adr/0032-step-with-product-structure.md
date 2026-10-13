# ADR-0032: STEP with product structure

- Status: Accepted
- Date: 2026-10-12

## Context

C1 imported a STEP file's solids as bodies and exported bodies alone.
Assemblies (ADR-0029) and the exchange unit (ADR-0028) make the product
tree the next step: a vendor's assembly should arrive as parts and
components, and an ArriX assembly should leave as a tree. Arris 0.6 reads a
`ProductTree` (`Occurrence`, `Rgb`, `FaceColour`) and writes one with
`write_products`; A12 gives the writer a unit (Arris `main`, unreleased).

The reader returns each solid baked at its world placement. Step 1's probe
(`crates/arrix-kernel/tests/probe_c2_m3.rs`) moved bodies back into their
product's frame by the inverse of the composed placement through
`ops::transform`: checker-green at `Level::Full`, vertices within 2e-17 m
of the part as built, volume within 5e-16 relative, a part's two
instances 3e-17 m apart, their `Role::File` entities identical and only
the instance differing. Eight orders under the kernel's 1e-6 m tolerance.

## Decision

1. **A product's frame by the inverse placement; no Arris ask (A13).** The
   cost is that a part placed n times is read n times, only the first
   kept. Decided by the human, 2026-10-12.
2. **Import shape.** One `Group` command (an undo removes it all) holding
   `AddFile`, then per product one `AddPart` with one `core.import-step`
   feature per solid in the product's own frame (`frame: product`), per
   assembly product one `AddAssembly`, and per occurrence one component
   grounded at its placement in its parent's frame. A product placed twice
   is one part used by two components. Names are `import.<feature>.
   <entity>` at instance 0, so a joint anchors on any instance's face.
3. **`core.import-step` type version 2** adds `frame: world | product`; a
   v1 record migrates to `world`, its current behaviour (ADR-0029).
4. **Colours become materials.** A distinct colour is one material
   (`appearance` set, `density` unset), assigned to the part (ADR-0031);
   the mass of such a part is `mass.no-material`-partial until the user
   gives a density. A per-face colour beyond the solid's is not kept.
5. **Export mapping.** A part is a product written once; a component is an
   occurrence at its solved pose relative to its parent assembly; a nested
   assembly is a sub-assembly product; final bodies are the product's
   solids; the material's appearance is its colour. Written in metres
   through `write_products_with` (A12, ADR-0028). Until Arris releases it
   `kernel.export.unit` stays.
6. **What is lost each way**: joints (the file carries placements only; an
   imported assembly is grounded components, an exported one writes its
   solved poses), textures, transparency, and component names, since
   Arris's `Occurrence` carries the product's name only. Accepting the
   component-name loss rather than asking Arris for the
   `NEXT_ASSEMBLY_USAGE_OCCURRENCE` name is plans/c2-m3 Open question 5,
   still the human's, by step 10.
7. **Sheet bodies** (Arris 0.8's surface-model reader) are refused at
   import as `kernel.import.sheet`.
8. **Surfaces**: a new `arrix import <step> -o <dir>` verb; `arrix export
   --format step [--assembly <id>]`; the app's `Import…` and `Export…`.

## Consequences

- A vendor assembly arrives with its structure, repeated parts shared.
- ArriX's own round trip is exact to rounding in every coordinate and name.
- Component names (`left screw`) do not survive the round trip until
  Open question 5 is answered.
- The importer and exporter are the core ones through the public plugin
  surface (`docs/PLUGINS.md`), no private hooks.

## Alternatives considered

- **Ask Arris to read each product's solids once in the product's frame
  (A13)**: saves reading a repeated part n times, at the price of waiting
  on a release for a cost the probe shows is only work.
- **Import every solid at its world placement, one part per file**: loses
  reuse and makes a vendor assembly one inert lump.
- **One material per face colour**: a material is a part-level fact; a
  face colour is a look.
- **Write millimetres**: ADR-0028 §5.

## Update 2026-10-13

The human answered plans/c2-m3 Open question 5 in §6: the component-name
loss is accepted. An imported component is named for its product, and an
exported one is an occurrence of its part's or assembly's name. A per-body
material override is not written either: an occurrence's colour is the
part's material's appearance. Nothing above is superseded.
