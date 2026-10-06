# Plan: c1-m2-naming

- Started: 2026-10-03
- Cycle: C1, M2 (the first of its two plans; the second, the part features
  and projection, follows it)
- Idea: docs/ideas/naming-strategy.md (absorbed)

## Goal

At the kernel layer, every operation M2's features need is named from Arris's
provenance, and the names hold. `arrix-kernel` offers `revolve`, `fillet`,
`chamfer`, `transform`-based copies (the pattern and mirror instances) and
`mirror`, each returning a body whose faces, edges and vertices have exactly
one name. A modification that only trims an entity keeps its name; only a
split piece `k > 0`, or an entity the operation made, gets a new one. Copies
carry an instance word, so a pattern's count can rise and fall without
renaming the copies that stay. A failed call's record exports as an Arris
fixture recipe, from `arrix eval --save-fixtures`, and an oversize fillet
fails as `kernel.degenerate.blend-too-large`. Kernel-level naming scenarios
(an edit, then a check that a name still names the same entity; the opposite
case, then a name that names nothing) pass.

## Non-goals

- The document-side features (`core.revolve`, `core.fillet`, `core.chamfer`,
  `core.hole`, the patterns, `core.mirror`, `core.boolean`), their forms and
  the document-level scenarios: plan `c1-m2-features`.
- Projection of edges and vertices into a sketch: plan `c1-m2-features`.
- A forward-closure resolver (idea option C): a later ADR if B proves too loose.
- Any change to `arrix-plugin-api`'s WIT: a name is opaque text there.
- Multi-tool booleans (`cut_many`, `fuse_many`: on Arris's `main` for 0.6,
  unreleased), `region2`: Arris backlog (`docs/ROADMAP.md` §C1 out).

## Design deltas

- **ADR-0010** (step 1), in the style of ADR-0007: the `mod` rule of
  `docs/DATA-MODEL.md` §Persistent naming changes so that piece `0` of a
  `Modified` origin keeps the origin's name and only pieces `k > 0` take
  `mod.<feature>.<k>`; the Instance step; the revolve root. Product
  scenario: a hole on a plate's top face survives a fillet added, removed or
  reordered above it. It supersedes the `mod` paragraph of that section, not
  ADR-0007.
- `arrix-core`: `NameStep::Instance { feature, index }` (text
  `inst.<ID>.<N>`), a `NameRoot::Revolve { feature, part }` (text
  `revolve.<ID>.<part>`) sharing `SweepPartName`; parser, printer and serde
  updated; the `gen` nesting limit applies to both. The DAG reads the
  feature an `Instance` step names, like `mod` and `gen`.
- `arrix-kernel`: `naming.rs` gains the B rule, a `revolve_names`, a
  blend-names function (generalising `boolean_names` to one operand and
  one record), and instance naming; `lib.rs` gains `revolve`, `fillet`,
  `chamfer`, `copy`, `mirror`; `record.rs` gains the matching `KernelOp`
  variants and a fixture-recipe export.
- `arrix-cli`: `eval --save-fixtures <dir>` (`docs/ARCHITECTURE.md` §The
  kernel choke point, "Fixture export", which moves from M4 to here).
- Stored form: a name's text grammar gains two words, additive. A name stored
  as `…/mod.<F>.0` by M1 no longer exists after step 4; see the first open
  question for what happens to M1's own scenario documents and goldens.
- `arrix-plugin-api`: unchanged (names are opaque `persistent-ref` text). The
  API doc's grammar line is updated; semver effect none.

## Arris dependencies

All present in `arris` 0.5.0, taken in step 3; none is an ask. 0.5.0 breaks
nothing ArriX calls: the workspace builds unchanged, M1's frozen results and
the slice's goldens hold, and of the probe only the second fillet's refusal
moves (`TangentChain` becomes `BlendTooLarge`, step 3). 0.4.0 was checked
against the same list at step 2.

- `ops::revolve` and `Role::Revolve(SweepPart)`, with `Cavity` for a void
  shell (`docs/DATA-MODEL.md` §Provenance in Arris).
- `ops::fillet`, `ops::chamfer`: blend entities `Generated` from the blended
  edge, adjacent faces and shortened edges `Modified`, error reasons
  `BlendTooLarge`, `TangentChain`, `VertexBlend`.
- `ops::transform`, `ops::mirror`: every entity `Modified` one-to-one
  (`mirror` is A11, in `arris` 0.4.0, Arris's C5).
- Arris's C6 blend table, released in 0.5.0: besides plane–plane and
  plane–cylinder, a cone against a plane, cylinder or cone and a sphere or
  torus against a coaxial plane, cylinder or cone; tangent chains (naming
  one edge of a slot's rim or a stadium blends the whole chain, one blend
  face per edge); a rim split into arcs; corners of unequal dihedrals (two
  pieces and a trim arc); blends that run to a step or a split face across.
  A pair outside the table still fails soft as `kernel.unsupported.*`; a
  blend that runs off its face as `kernel.degenerate.blend-too-large`.
  These change step 7: a feature naming one edge gets blend faces from
  edges it did not name.
- A gap met by the probe (step 2): a 2 mm hole at (39, 15) mm through a
  plate whose top edge at x = 40 mm is filleted at 3 mm, crossing the
  fillet face and the side face below it, fails in `arris` 0.4.0's `cut`
  as `Internal(Split(Dangling))`, on an extrude and on a primitive box
  alike; at x = 38.5 and 41 it succeeds. Still so in 0.5.0. Pinned by
  `a_hole_dragged_across_a_fillet_leaves_the_top_face_one_piece`; it goes
  to Arris as a fixture from step 9's export and a backlog line there,
  which the human files (the Arris checkout is read-only here).

## Steps

- [x] **[1]** Step 1 — ADR-0010: the single-piece rule, the Instance step and
  the revolve root, with the scenario above. Update `docs/ROADMAP.md`'s A11
  line (landed in `arris` 0.4.0, `core.mirror` in M2) in the same commit.
- [x] **[3]** Step 2 — probe tests in `arrix-kernel/tests/` (the scenario's
  own units, metres): what Arris records for `revolve` (a stepped profile, a
  full turn and a partial one), `fillet` and `chamfer` (one edge, a chain, a
  corner of three), `transform` and `mirror`; and how piece `0` behaves under
  the edits of the accept (a hole dragged toward and across a face boundary,
  a pattern's count raised and lowered). If piece `0` flips under a plausible
  edit, stop: the idea's option C is back on the table and the human decides.
  *Done: `crates/arrix-kernel/tests/probe_m2.rs`. Piece `0` flips: see the
  first open question. Steps 3 onward wait on it.*
- [x] **[1]** Step 3 — take `arris` 0.5: the workspace requirement,
  `ARRIS_VERSION`, the lock; the second fillet's probe expects
  `BlendTooLarge` where 0.4.0 said `TangentChain`; `docs/ARCHITECTURE.md`'s
  pin line. Nothing else moves (checked 2026-10-06 in a scratch worktree:
  every other test green, the slice's goldens included).
- [x] **[2]** Step 4 — the B rule in `boolean_names`: piece `0` keeps the
  origin's name; pieces `k > 0` take `mod`; `docs/DATA-MODEL.md` states
  what a split piece's reference follows (first open question). M1's expectations in
  `booleans.rs` and `gear_naming.rs` and the slice's goldens change; their diff
  is read, not refreshed blind (`docs/DATA-MODEL.md` §Persistent naming is
  updated in the same commit).
- [x] **[2]** Step 5 — `NameStep::Instance` and `NameRoot::Revolve` in
  `arrix-core`: text form, parse errors naming the byte, serde, `proptest`
  round trip of the grammar, the DAG's reads.
- [x] **[2]** Step 6 — `revolve` in `arrix-kernel`: the call record, the
  names from `Role::Revolve` through the profile's curve keys, a full turn
  (no end cap, seams as start edges) and a cavity, each exactly one name per
  entity.
- [ ] **[2]** Step 7 — `fillet` and `chamfer`: names for the blend face, its
  contact edges, ends and corner entities from `gen.<F>[…]` of the edge
  names; the trimmed neighbours keep theirs by the B rule; `BlendTooLarge`,
  `TangentChain`, `VertexBlend` map to their `kernel.degenerate.*` codes
  with the record kept. Includes a second blend on a blended body. Begins
  by probing 0.5.0's provenance for a tangent chain named by one edge (is
  each chain edge the origin of its own blend face?) and for an unequal
  miter (what its trim arc and the third edge's end come from); a blend
  face from an edge the feature did not name takes `gen.<F>[<that
  edge's name>]`, so the names stay one per entity whichever edge of the
  chain was picked.
- [ ] **[2]** Step 8 — instances: `copy` (a body transformed `N` times, each
  entity `inst.<F>.<i>`, `i` from 1) and `mirror` (instance 1), then the
  pattern shapes as a fuse or cut of the copies. Raising or lowering `N`
  renames no surviving copy.
- [ ] **[3]** Step 9 — the fixture export: a `KernelCall` serialises as an
  Arris `fixture.json` (the operands' closures as body bytes, the operation,
  the precision, nothing else), read first against Arris's fixture format and
  runner. A golden recipe for the oversize fillet.
- [ ] **[2]** Step 10 — `arrix eval --save-fixtures <dir>`: each failed
  feature's record written as a recipe, one directory per failure, named by
  its diagnostic code and a content hash; `tests/docs/` gains a document with
  an oversize fillet kept as a plain text document (no feature type yet:
  built by the kernel test's own driver if `core.fillet` is not there).
- [ ] **[2]** Step 11 — the kernel naming scenarios (the acceptance below)
  and their opposite cases, as one test file.

Each step is one commit; steps 2 and 9 begin as probes and may rewrite the
steps after them.

## Acceptance

`cargo test -p arrix-kernel --test naming_scenarios` and `cargo test -p
arrix-cli --test save_fixtures`, with the gate green. The first holds, on a
plate with a through hole and a fillet on a plate edge: a width edit under
the fillet, the hole moved across the plate, a pattern count raised and
lowered, a sketch curve redrawn between two kept ones, a feature inserted
before the filleted edge, a fillet added, removed and reordered above a
reference to the top face; each ends with the same name naming the same
entity (checked by the name table, never by geometry). Each opposite case (the
referenced edge removed) leaves the name absent from the table. The second
runs `eval --save-fixtures` on the oversize fillet: the diagnostic is
`kernel.degenerate.blend-too-large` and the recipe it writes parses as an
Arris fixture and, in the check described under the open questions, fails
the same way in Arris.

## Docs to update on completion

- `docs/DATA-MODEL.md` §Persistent naming — the single-piece rule, the
  Instance step, the revolve root, the grammar block; the blend and
  mirror paragraphs; the resolution paragraph's mention of `mod`.
- `docs/ARCHITECTURE.md` §The kernel choke point — the operations built, the
  fixture export (built, not "M4, not built"), the failure categories added.
- `docs/PLUGINS.md` §Naming — the grammar line.
- `docs/ROADMAP.md` — M2's status line and the A11 row (landed); the risk row
  "Names do not hold through booleans and patterns" notes the kernel half
  retired.
- `docs/BACKLOG.md` — remove the "feature that sweeps twice names both caps
  alike" line, absorbed (an `index` in the sweep root if a boolean ever needs
  it, the hole being one revolved profile).
- `AGENTS.md` current state — the plan landed, `c1-m2-features` next.

Written at planning time; executed before the plan is deleted.

## Open questions

- Decided 2026-10-03 (step 2's finding; the human chose (a)): **piece `0`
  of a split face flips under a plausible edit, and ADR-0010 stands.** The
  probe holds every single-piece case ADR-0010 is for: a blend, a copy and
  a mirror split nothing, a hole dragged across a fillet leaves the top
  face one piece, a pattern's holes come each from its own copy at any
  count. But when a face is split in two, Arris orders the pieces by the
  ids of the entities bounding them (its ADR-0009), an extrude numbers a
  profile's sides in walk order, and the sketcher starts a loop's walk at
  its oldest entity: the same plate walked from corner 2 or 3 instead of 0
  or 1 swaps the pieces (`piece_0_of_a_split_follows_the_profile_s_walk`),
  so redrawing the two oldest sides of a rectangle moves the bare name to
  the other piece. M1's `mod.<F>.0` flips the same way. Chosen: (a) keep
  the B rule and state in `docs/DATA-MODEL.md` (step 4) that a reference to
  one piece of a split face follows Arris's order, which a redraw of the
  curves bounding the pieces can change; the acceptance's "curve redrawn
  between two kept ones" redraws no side that bounds one piece alone. The
  real fix is (c), an Arris ask: the boundary key compared by a
  consumer-given order (its `ConsumerKey`, ADR-0028 there) instead of input
  ids; it is a row of `docs/ROADMAP.md` §C1 Arris dependencies, filed in
  Arris's asks by the human. Not chosen: (b) the idea's option C, a
  forward-closure resolver.
- Decided 2026-10-06 (the human chose A): no schema bump. Step 4 edited
  `tests/docs/slice/parts/F7Y3N3HJPEEDE.json` (the pocket sketch's
  reference lost its `/mod.<join>.0`; before the edit `arrix eval` said
  `ref.lost` and offered the bare face) and the tests; the `eval` goldens
  hold unchanged, since they carry no names.
- Found in step 6: a void's `Cavity` role generates a *shell*, which is no
  face, edge or vertex and has no name; `sweep_names` skips it, and the
  void's walls are named by their own `Side`, `Rise` and edge roles
  through the cavity loop's keys (`crates/arrix-kernel/tests/revolve.rs`).
  `Kernel::revolve` takes the axis as a world point and direction; the
  host does not offer it to plugins yet (plugin API: step of
  `c1-m2-features`, with `core.revolve`).
- Found in step 4: with piece `0` bare, a feature that sweeps twice and
  fuses the sweeps gave two entities one name (`demo.stack` in
  `arrix-plugin-host/tests/tier0.rs`, `kernel.naming`). The human chose a
  sweep index in the sweep root: ADR-0011. `NameRoot::Sweep` has `index`
  (0 not written), the host counts a feature's sweeps, `Kernel::extrude`
  takes it. Step 5's revolve root takes the same `index`; step 6's
  `revolve` is counted with the extrudes. The backlog line is removed.
- ⚠ OPEN: how "reproduces in Arris's fixture runner" is checked. Preferred:
  the recipe is validated in the test against a copy of Arris's fixture
  schema vendored under `tests/` (no `path =` dependency), and the real runner
  is run once by hand against `docs/notes/LOCAL.md`'s checkout, the result
  recorded in the commit message. Who: human, by step 9.
- ⚠ OPEN: pattern shape, one fused tool and one cut, or `N` cuts. Preferred:
  one fused tool, since Arris handles multi-lump solids (C2) and the name
  of a hole's wall then carries one `inst` step, not a chain. Arris's
  `cut_many` (on `main` for 0.6, unreleased) is a third shape: one boolean
  with one record naming each tool, every untouched entity keeping its id.
  If 0.6 is released before step 8, it is taken there with its breaking
  `Reason` regrouping; otherwise the fused tool, and `cut_many` a later
  bump. Who: agent,
  by step 8, after the probe of step 2.
