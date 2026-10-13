# ADR-0034: C3 is the boundary crossed

- Status: Accepted
- Date: 2026-10-13

## Context

`SEED.md` §7 gives C3 four items: the Tier 1 (wasm component) host,
robotics "moved out as plugin #1", a second first-party plugin in Python
through Tier 2 only, and `arrix-plugin-api` reviewed for 0.x stability.
Its accept is the north-star demo. Measured against the tree at C2's close,
that line has four problems:

- **There is no robotics plugin to move.** `plugins/` holds `gears` alone.
- **The demo does not need Tier 1.** The fasteners are Python (Tier 2,
  built), and robotics could run at any tier. What the demo does need is
  API the world lacks: the WIT exports `feature` only. A URDF exporter
  reads an evaluated assembly (contribution points 4 and `document-read`),
  and placing fasteners into an assembly adds parts, components and joints
  (contribution point 3, whose undo ADR-0004 already decided).
- **A stability review before exporters and commands exist** would review
  an API about to grow by half.
- **"Moved out" means repositories** under `arris-labs` (ADR-0003) and a
  published `arrix-plugin-api`, which are the human's acts.

The core already has what robotics reads: joints with limits, URDF's
roll, pitch, yaw poses (ADR-0029), and mass, centroid and inertia per body,
part and assembly (ADR-0031). The real unknown is the cycle's name: does the
plugin API hold across every boundary? A Rust plugin has to run sandboxed as
a component exactly as it does compiled in, and two real domains have to
need nothing but the public API.

## Decision

C3 keeps its name and its accept. Its scope is the following, in three
milestones, risk first:

1. **M1, Tier 1.** A wasmtime host with the component model, native only.
   A component's imports are the world's and nothing else. Fuel is the
   deterministic step budget, and an epoch interrupt cancels. The Rust
   guest SDK is a feature of `arrix-plugin-api`, so a plugin still depends
   on that crate alone, and an ADR places the `unsafe` the guest bindings
   need. `gears` built as a component evaluates identically to `gears`
   compiled in. Tier 1 comes first so that every later API addition lands
   on three tiers at once, not on two with the third catching up.
2. **M2, a plugin reads an assembly.** A snapshot read imported by the
   world under `document-read` (parts, final bodies' meshes, assemblies
   with solved poses, joints, mass and inertia). An `exchange` export: an
   exporter takes a snapshot and gives bytes and a file name. Both land on
   Tiers 0, 1 and 2. `robotics`, first-party plugin #1, is new and written
   in Rust, and it runs as a Tier 1 component in the accept. It exports
   URDF: a link per component, its joints with their limits, meshes as
   STL, and inertials from the document's materials.
3. **M3, commands, fasteners and the accept.** The `commands` export:
   the plugin expands a gesture into one `Group` of core commands
   (ADR-0004), bound to a ribbon button and the selection, through the
   protocol and Tier 2. `fasteners`, first-party plugin #2, is Python
   through Tier 2 only. It places socket-head cap screws from one ISO 4762
   table into the picked holes: each size a part whose history is a
   `fasteners.*` feature, each screw a component held by a fixed joint
   anchored on the hole's edge, so a width edit moves it. Out of tree,
   proven in tree: each first-party plugin becomes a Cargo workspace of
   its own, depending on `arrix-plugin-api` alone and passing `arrix test`
   from the installed binary, with `arrix-plugin-test`, the Rust kit for
   Tier 0 and Tier 1 authors. An ADR then reviews `arrix-plugin-api` for
   0.x stability.

The URDF is held to an independent reader's reading: urdfdom (the parser
RViz uses) and a Python URDF loader, run by a script never in CI, into a
committed text golden. That is how C2's STEP was held to Open CASCADE's.

Moving the plugins into their own repositories and publishing
`arrix-plugin-api` stay the human's acts, after the accept. `SEED.md` §9's
row ("in-tree until C3, then their own repositories") stands with that
timing.

`SEED.md` is amended: §7's C3 line takes this scope, and §6.7's
`plugins/robotics` line becomes Tier 1. `docs/ROADMAP.md` §C3 is
rewritten with the milestones.

## Consequences

- The cycle retires its unknown before any domain code exists. If the
  component model cannot carry the kernel's handles, or native and wasm
  math differ, M1 finds out on a plugin that already works.
- The API grows by two imports and two exports, each on three tiers, each
  held by the equality test and the generated wire. Its stability review
  then reviews the API that third parties will meet.
- Fasteners are assembly-correct: they are components in a bill of
  materials, follow the holes through joints, and freeze as their part's
  feature when the plugin is absent.
- Plugin document data (`PluginData`, record nodes) is still unbuilt. Its
  first need is a robotics setting with no home in the core, such as a
  joint's effort and velocity, which the URDF exporter takes as options
  for now.
- Robotics is unavailable in the browser until Tier 1 is (C4's ADR). It
  contributes no feature, so no document freezes for lack of it.

## Alternatives considered

- **Fasteners as a feature in the bracket's history**: this fits today's
  API with one new input kind, but the screws become bodies of the
  bracket, not components, and contribution point 3 stays unproven.
- **API first, Tier 1 last**: the demo arrives sooner, but Tier 1 then
  catches up with a world twice the size, and the cycle's named risk is
  retired last.
- **Splitting the repositories inside the cycle**: this needs repositories
  created and the API published mid-cycle, for a proof the in-tree
  separate workspaces give.
- **Deferring the move out to a named cycle**: it leaves `SEED.md` §9's
  promise unkept with no proof that the out-of-tree path works.
