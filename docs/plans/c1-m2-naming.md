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
- Multi-tool booleans, `region2`: Arris backlog (`docs/ROADMAP.md` §C1 out).

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
  as `…/mod.<F>.0` by M1 no longer exists after step 3; see the first open
  question for what happens to M1's own scenario documents and goldens.
- `arrix-plugin-api`: unchanged (names are opaque `persistent-ref` text). The
  API doc's grammar line is updated; semver effect none.

## Arris dependencies

All present in the pinned `arris` 0.4.0; none is an ask.

- `ops::revolve` and `Role::Revolve(SweepPart)`, with `Cavity` for a void
  shell (`docs/DATA-MODEL.md` §Provenance in Arris).
- `ops::fillet`, `ops::chamfer`: blend entities `Generated` from the blended
  edge, adjacent faces and shortened edges `Modified`, error reasons
  `BlendTooLarge`, `TangentChain`, `VertexBlend`.
- `ops::transform`, `ops::mirror`: every entity `Modified` one-to-one
  (`mirror` is A11, in `arris` 0.4.0, Arris's C5).
- Plane–plane and plane–cylinder edges only (Arris ADR-0007); a blend on a
  pair outside that table fails soft as `kernel.degenerate.*` or
  `kernel.unsupported.*`. Arris's own C6 widens the table; ArriX takes it
  with a version bump, not a workaround.
- A gap met by the probe (step 2): a 2 mm hole at (39, 15) mm through a
  plate whose top edge at x = 40 mm is filleted at 3 mm, crossing the
  fillet face and the side face below it, fails in `arris` 0.4.0's `cut`
  as `Internal(Split(Dangling))`, on an extrude and on a primitive box
  alike; at x = 38.5 and 41 it succeeds. Pinned by
  `a_hole_dragged_across_a_fillet_leaves_the_top_face_one_piece`; it goes
  to Arris as a fixture from step 8's export and a backlog line there,
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
- [ ] **[2]** Step 3 — the B rule in `boolean_names`: piece `0` keeps the
  origin's name; pieces `k > 0` take `mod`. M1's expectations in
  `booleans.rs` and `gear_naming.rs` and the slice's goldens change; their diff
  is read, not refreshed blind (`docs/DATA-MODEL.md` §Persistent naming is
  updated in the same commit).
- [ ] **[2]** Step 4 — `NameStep::Instance` and `NameRoot::Revolve` in
  `arrix-core`: text form, parse errors naming the byte, serde, `proptest`
  round trip of the grammar, the DAG's reads.
- [ ] **[2]** Step 5 — `revolve` in `arrix-kernel`: the call record, the
  names from `Role::Revolve` through the profile's curve keys, a full turn
  (no end cap, seams as start edges) and a cavity, each exactly one name per
  entity.
- [ ] **[2]** Step 6 — `fillet` and `chamfer`: names for the blend face, its
  contact edges, ends and corner entities from `gen.<F>[…]` of the edge
  names; the trimmed neighbours keep theirs by the B rule; `BlendTooLarge`,
  `TangentChain`, `VertexBlend` map to their `kernel.degenerate.*` codes
  with the record kept. Includes a second blend on a blended body.
- [ ] **[2]** Step 7 — instances: `copy` (a body transformed `N` times, each
  entity `inst.<F>.<i>`, `i` from 1) and `mirror` (instance 1), then the
  pattern shapes as a fuse or cut of the copies. Raising or lowering `N`
  renames no surviving copy.
- [ ] **[3]** Step 8 — the fixture export: a `KernelCall` serialises as an
  Arris `fixture.json` (the operands' closures as body bytes, the operation,
  the precision, nothing else), read first against Arris's fixture format and
  runner. A golden recipe for the oversize fillet.
- [ ] **[2]** Step 9 — `arrix eval --save-fixtures <dir>`: each failed
  feature's record written as a recipe, one directory per failure, named by
  its diagnostic code and a content hash; `tests/docs/` gains a document with
  an oversize fillet kept as a plain text document (no feature type yet:
  built by the kernel test's own driver if `core.fillet` is not there).
- [ ] **[2]** Step 10 — the kernel naming scenarios (the acceptance below)
  and their opposite cases, as one test file.

Each step is one commit; steps 2 and 8 begin as probes and may rewrite the
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

- ⚠ OPEN (step 2's finding, blocks step 3): **piece `0` of a split face
  flips under a plausible edit.** The probe holds every single-piece case
  ADR-0010 is for: a blend, a copy and a mirror split nothing, a hole
  dragged across a fillet leaves the top face one piece, a pattern's holes
  come each from its own copy at any count. But when a face is split in
  two, Arris orders the pieces by the ids of the entities bounding them
  (its ADR-0009), an extrude numbers a profile's sides in walk order, and
  the sketcher starts a loop's walk at its oldest entity: the same plate
  walked from corner 2 or 3 instead of 0 or 1 swaps the pieces
  (`piece_0_of_a_split_follows_the_profile_s_walk`), so redrawing the two
  oldest sides of a rectangle moves the bare name to the other piece.
  Widths, an unrelated body, an earlier hole or boss, and the tool made
  first do not move it. M1's `mod.<F>.0` flips the same way, so the B rule
  makes this no worse; it was unseen until now. Options: (a) keep B and
  accept it for split faces, stated in `docs/DATA-MODEL.md`: a reference to
  one piece of a split face follows Arris's order, which a redraw of the
  curves that bound the pieces can change (starting each loop at its
  lowest `CurveKey` would not help: the oldest entity is already the
  lowest key); the acceptance's "curve redrawn between two kept ones" then
  redraws no side that bounds one piece alone; (b) the idea's option C, a
  forward-closure resolver; (c) an Arris ask, the boundary key compared by
  a consumer-given order (its `ConsumerKey`, ADR-0028 there) instead of
  input ids, which with keys from the sketch makes a redraw flip only the
  pieces it really re-bounds. Preferred: (a) now, (c) as a line in Arris's
  asks. Who: human, before step 3.
- ⚠ OPEN: M1's scenario documents (`tests/docs/slice`) and the goldens
  contain `mod.<F>.0` names. Preferred: no schema bump, because C1 is
  unreleased; the documents and goldens are regenerated in step 3 and the
  diff read. Who: human, by step 3. A schema 3 with a migration rewriting
  `…/mod.<F>.0` is the alternative.
- ⚠ OPEN: how "reproduces in Arris's fixture runner" is checked. Preferred:
  the recipe is validated in the test against a copy of Arris's fixture
  schema vendored under `tests/` (no `path =` dependency), and the real runner
  is run once by hand against `docs/notes/LOCAL.md`'s checkout, the result
  recorded in the commit message. Who: human, by step 8.
- ⚠ OPEN: pattern shape, one fused tool and one cut, or `N` cuts. Preferred:
  one fused tool, since Arris handles multi-lump solids (C2) and the name
  of a hole's wall then carries one `inst` step, not a chain. Who: agent,
  by step 7, after the probe of step 2.
