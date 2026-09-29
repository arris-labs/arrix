# Plan: c1-m1-slice

- Started: 2026-09-27
- Cycle: C1, milestone M1 (`docs/ROADMAP.md` §M1), third and last of its
  three plans
- Idea: none; verbatim from the human: "/plan c1-m1-slice"

`c1-m1-document` proved a plugin feature on the document's one path, and
`c1-m1-sketch` gave it sketched regions. This plan closes M1: bodies that
features modify, frozen results, the `.arrx` zip and migrations, and M1's
accept scenario. The frozen steps wait on Arris ask A2 (body bytes), so
they come last, as the roadmap's risk register orders them.

## Goal

A part is built by commands alone, headless: a sketched plate extruded, a
`gears.spur` on a datum plane above it joined to it, and a pocket sketched
on the joined body's top face and cut through both. A feature reads a body
as the version current at its place in the history, so a join or a cut
makes a new version of its target's slot, and the DAG holds the edges
that implies. Names run through `fuse` and `cut` from Arris's provenance
(`mod` and `gen` steps), and editing the plate's width re-evaluates
everything after it with every reference resolving. Every successful live
evaluation of a plugin feature stores its result as body bytes and a name
table in content-addressed blobs. Without the plugin the gear evaluates
frozen, with the reason *missing*, and everything after it evaluates
against the stored result with the same volumes. An upstream edit makes it
frozen-stale, and it is re-frozen when the plugin returns. `.arrx` is
written and read as a directory and as a deterministic zip, at schema 2,
with a migration chain whose first link reads schema 1.

## Non-goals

- `core.revolve`, fillet, chamfer, hole, patterns, mirror: M2.
  `core.boolean` comes in only as far as the gear's join needs (OPEN 1).
- Naming scenarios beyond the accept (a hole moved, a pattern count
  raised, each with its opposite case), ranked candidates through
  booleans, and projection: M2. This plan names through `fuse` and `cut`
  and resolves the accept's references.
- Kernel primitives: no M1 feature calls one. They join with the first
  feature that does, and the roadmap's M1 in-list says so at retirement.
- Fixture export and `--save-fixtures` (M4), though call-record operands
  become content hashes once body bytes exist.
- Plugin document data (`PluginData`, `plugins/<id>/`): no M1 scenario
  writes it.
- Tiers 1 and 2, the frozen state's "tier cannot run here" reason (no
  tier but 0 exists), any UI badge, tessellation.
- Rescaling or migrating body bytes: Arris owns that (its ADR-0029).

## Design deltas

- **ADR-0007, bodies as inputs and modified slots** (step 3): a feature
  type may declare a `body` input, which resolves to the body its
  `Ref::Slot` names *as of the reading feature's place in the history*,
  and an output slot that **modifies** one of its body inputs. That slot is
  the target slot's next version, not a slot of its own
  (`docs/DATA-MODEL.md` §Bodies across features). A body input read as a
  tool and not modified ends its slot (`slot.consumed` downstream). The
  kernel interface gains `fuse` and `cut`. The DAG gains the implicit
  edges: a feature reading slot `s` reads every feature before it that
  modifies `s`. Built-ins and plugins use the same words (no doc-level
  extension like `sketch_slots`), because `core.extrude` evaluates through
  the plugin API's `Kernel` like everything else.
- **`arrix-plugin-api` 0.3.0** (API: minor, adds `input-kind.body`,
  `input-value.body(body)`, an owned handle since `wit-bindgen` 0.62
  cannot generate the borrow inside a list (ADR-0008, found in step 4),
  `slot-spec.modifies: option<string>`, `kernel.fuse` and `kernel.cut`;
  nothing removed). The WIT equality test is extended. `plugins/gears`
  declares `api = "^0.3"`, and its one `SlotSpec` gains `modifies: None`.
- **`arrix-kernel`**: `fuse` and `cut` (target, tool) with call records;
  names through their provenance: a kept entity keeps its name, a piece
  is `mod.<feature>.<k>` of its origin with `k` its index in Arris's
  split order, a generated entity is `gen.<feature>[…]` of its origins'
  names, sorted. An entity with no nameable origin, or two entities with
  one name, is refused as `kernel.naming`. Then, with A2: `to_bytes` and
  `from_bytes` of a body with its name table, and call-record operands as
  body content hashes.
- **`arrix-doc`**: `core.extrude` (a `region` input, `distance`, a `mode`
  choice `new`/`join`/`cut`, a `target` body input for join and cut),
  `core.boolean` for the join (OPEN 1), body-slot versions in resolution
  and the DAG, `FrozenResult` and the frozen states in the evaluator, the
  blob store, the zip, the migration chain.
- **ADR-0009, where frozen results are written** (step 9), proposed: a
  frozen result is evaluation output, not a command. The session keeps the
  latest live result per plugin feature, keyed by its input hash, beside
  the document, as it keeps generation stamps. `save` writes it into the
  feature's record and blobs, and otherwise keeps what the document was
  opened with. It has no undo entry and no generation, and it is never
  stale-rejected. Evaluation is deterministic, so undo, re-evaluate and
  save gives the earlier bytes (OPEN 2).
- **Schema 2**: `FeatureRecord.frozen` and `blobs/` are new persisted
  shapes, so the schema goes from 1 to 2 (`docs/DATA-MODEL.md` §File
  format, "1 at C1" becomes 2). Migration 1 → 2 is the identity on the JSON
  tree bar the header. Its fixture is today's `tests/docs/gear-on-plane/`,
  copied as it stands to `tests/docs/migrations/schema-1/`, opened and
  re-saved to the schema-2 bytes it must give.
- **`.arrx` zip**: deterministic (sorted entries, a fixed timestamp, JSON
  deflated, blobs stored). The zip crate is chosen in step 11 by what
  builds for wasm32 without C and writes byte-identical output (OPEN 3).
  `arrix eval` reads either form, and `arrix eval --without-plugin <id>`
  (repeatable) evaluates with that plugin left out of the registry.
- **Diagnostics**: `plugin.frozen.missing` (it exists in the code list),
  `plugin.frozen.version`, the stale warning `plugin.frozen.stale`
  propagated downstream as a warning, `slot.consumed`,
  `extrude.no-target`, `blob.missing`, `blob.corrupt`, `doc.migration`.

## Arris dependencies

- **Present in the pinned `arris` 0.2.0**: `ops::fuse(&mut Model, Body,
  Body)` and `ops::cut(&mut Model, Body, Body)`, each `-> (Body,
  Provenance)` (`arris-ops` `boolean/mod.rs`), on plane, cylinder and the
  other analytic faces. B-spline operands are refused, and none arises
  here. The touching case (two operands meeting along a face, the gear's
  bottom on the plate's top) is decided, not refused. `Provenance::
  modified_from`, `generated_from` and `origins`, with the split order a
  contract (Arris ADR-0009).
- **Released in `arris` 0.3.0, taken in step 1**: nothing this plan needs.
  Its breaks (`Role::File`, a mass-properties integration that moves the
  last bits) are taken early, so the A2 bump later carries only A2's.
- **Ask A2, body bytes: not released.** Arris's plan `body-bytes` (its
  ADR-0029) has steps 1–3 of 8 done in `arris` 0.4.0-dev:
  `arris_io::body::write(&Model, Body, &Provenance) -> Vec<u8>` and
  `read(&mut Model, &[u8]) -> Imported`, deterministic, with a v1 guard
  per version. Steps 12–15 wait for a released `arris` carrying it. 0.4
  also breaks `Role` (`Consumer`) and `OpError` (`Unkeyed`, `Rejected`),
  taken in step 12. Nothing is worked around meanwhile: no body bytes of
  ArriX's own and no re-evaluation standing in for a frozen result.

## Steps

Complexity: **[1]** routine; **[2]** careful, a case to get right within
a given design; **[3]** unproven, behaviour to establish here.

- [x] **[2]** Step 1: `arris` 0.2 → 0.3. The `Role::File` arm in the
  naming (a file root is refused as `kernel.naming` until C2's import).
  Any golden whose last bits move is read, refreshed, and the commit says
  why (0.3's mass properties integrate about the body).
- [x] **[3]** Step 2: `arrix-kernel` probe and booleans. `Kernel::fuse` and
  `cut` with `KernelOp` records, and names through their provenance.
  Probe tests in SI: a 40 × 30 × 5 mm plate with a boss fused, then a
  pocket cut through it, with counts and volumes computed by hand; a
  20-tooth, 1 mm, 8 mm gear standing flush on the plate's top, fused (the
  touching case), then a pocket cut through both; each twice from fresh
  state to identical names and volumes. A `mod` step's `k` is the same
  after a plate-width edit that keeps which entities bound each piece.
  Every entity has one name and every name one entity. A kernel refusal
  here becomes an Arris fixture and a line in its backlog; this plan
  waits, it does not route around.
- [x] **[1]** Step 3: ADR-0007, bodies as inputs and modified slots
  (the design delta above), with `docs/DATA-MODEL.md` §Bodies across
  features, §The dependency DAG and `docs/PLUGINS.md` §One interface
  brought to it. Docs only.
- [x] **[2]** Step 4: plugin API 0.3.0. The WIT world and Rust traits:
  the body input kind and value, `slot-spec.modifies`, `kernel.fuse` and
  `kernel.cut`. The equality test is extended, the Tier 0 host adapts
  them, `gears` declares `api = "0.3"`, the wasm build is green.
- [x] **[2]** Step 5: body-slot versions in `arrix-doc`. A body input
  resolves to the version current at the reading feature. A modifying
  slot is the target's next version. A consumed slot fails its later
  readers with `slot.consumed`. The DAG gains the implicit edges. Tests
  use a test-registered modifier: inserting a modifier earlier moves every
  later reader to its version; reordering past a modifier is refused when
  it would break the order; undo of each restores the earlier bytes.
  Proptest over random modifier insertions: the DAG is acyclic and each
  reader's version is the last modifier before it.
- [ ] **[2]** Step 6: `core.extrude`. A `region` input, `distance`
  (signed, as the kernel's extrude), `mode` `new`/`join`/`cut`, and a
  `target` body input for join and cut (`extrude.no-target` without one,
  `input.wrong-kind` on a plane). It replaces `test.pad` in
  `crates/arrix-doc/tests/sketched_plate.rs`. Tests: a plate extruded, a
  boss joined, a pocket cut, volumes by hand, and a cache hit when nothing
  upstream changed.
- [ ] **[2]** Step 7: `core.boolean` in M1's form (OPEN 1): `target` and
  `tool` body inputs, `op` `fuse`/`cut`. The target's slot is modified and
  the tool's consumed. Tests: a plate and a separate extruded boss fused
  to the plate's volume plus the boss's; reading the boss's slot after is
  `slot.consumed`.
- [ ] **[2]** Step 8: the live slice. `crates/arrix-cli/tests/slice.rs`
  builds the accept's part by commands with `gears` registered: the
  sketched plate, a datum plane on its top face, the gear on it, the join,
  and a pocket sketched on the joined top face (a `face:…/mod.…` name) and
  cut through both. Editing `w` re-evaluates all of it and every reference
  resolves, including the pocket's plane through the join. The document
  is saved to `tests/docs/slice/` and checked as `gear-on-plane` is.
  `arrix eval tests/docs/slice` has a golden. Save twice gives identical
  bytes, and undo of every command restores each earlier save.
- [ ] **[1]** Step 9: ADR-0009, where frozen results are written (the
  design delta above, as OPEN 2 settles it), with `docs/DATA-MODEL.md`
  §Frozen results and §Commands and undo brought to it. Docs only.
- [ ] **[2]** Step 10: schema 2 and the migration chain. `FrozenResult` on
  `FeatureRecord` (omitted when absent), `BlobRef` and `blobs/`
  (content-addressed; unreferenced blobs dropped on save; `blob.missing`
  and `blob.corrupt` on open). Migrations are pure functions over the JSON
  tree, run in order on open (`doc.migration` names the one that failed).
  1 → 2 is the identity, with its fixture in
  `tests/docs/migrations/schema-1/`. Every scenario directory is
  re-saved at schema 2, its diff read. Tests use a synthetic blob, since
  body bytes wait on A2.
- [ ] **[2]** Step 11: the `.arrx` zip. A deterministic writer and a
  reader, `open` taking either form, `arrix eval` on a zip. Save twice
  gives identical zip bytes; unzip then save gives the directory's bytes;
  a zip with an entry outside the tree, or a duplicate, is refused. The
  wasm build is green.
- [ ] **[2]** Step 12, waits for A2: `arris` 0.3 → 0.4. The `Role::
  Consumer` and `OpError` arms. `Kernel::to_bytes(body)` gives Arris body
  bytes and a name table (each name to the entity's index in the written
  body). `from_bytes` loads them into the model as a `KernelBody` whose
  names are the table's, refusing a table that does not cover the body
  exactly one to one (`kernel.naming`). Call-record operands become body
  content hashes. Tests: the gear's body round-trips to the same names and
  bit-identical volumes; written twice, identical bytes; a body from 0.4's
  guard fixture reads.
- [ ] **[3]** Step 13, waits for A2: frozen results written. On each live
  success of a plugin feature, the evaluator hands out its `FrozenResult`
  (input hash, plugin version, a blob per body slot, the name table's
  blob); the session keeps the latest (ADR-0009); `save` writes it; open
  and a fresh evaluator load it. Tests: the slice saved after
  evaluation carries the gear's blobs; saved twice, identical; the gear
  evaluated twice from fresh state gives identical blob bytes; undo of
  every command, re-evaluated and saved, restores each earlier save.
- [ ] **[3]** Step 14, waits for A2: the frozen states. A plugin feature
  whose type the registry lacks, or whose plugin version is outside the
  record's range, evaluates **frozen** from its stored result, with
  `plugin.frozen.missing` or `.version`. Its current input hash (under
  the stored plugin version) against the stored one decides
  **frozen-stale**, with `plugin.frozen.stale` as a warning on it and on
  every feature downstream. With the plugin back it is live and re-frozen.
  References into it resolve through the stored name table.
  `arrix eval --without-plugin <id>`. Tests: each state and each
  transition, and a frozen feature with no stored result failing as
  `plugin.frozen.missing` with no outputs.
- [ ] **[1]** Step 15: the acceptance below, with `tests/docs/slice/`'s
  goldens: live, `--without-plugin gears`, and edited without it.

## Acceptance

`cargo test -p arrix-cli --test slice`, which is M1's accept (headless,
through `LocalSession`, `gears` registered unless stated):

1. Commands alone build a sketched plate extruded, a spur gear on a datum
   plane above it joined to it, and a pocket cut through both.
2. Editing the plate's width re-evaluates everything after it, and every
   reference resolves.
3. Save, save again: identical bytes, in both the directory and the zip
   form.
4. `arrix eval --without-plugin gears`: the gear frozen with reason
   *missing*, the pocket still cut, volumes identical to the live run.
5. The width edited without the plugin: the gear frozen-stale, the pocket
   re-evaluated against its stored result.
6. Reopened with the plugin: re-frozen, not stale.
7. The gear evaluated twice gives identical bytes.
8. Undo of every command in reverse returns the document, byte for byte,
   to each earlier state.

And `arrix eval` equals its golden on `tests/docs/slice/`, with and
without `gears`; `tests/docs/migrations/schema-1/` opens and re-saves to
its schema-2 bytes; the whole gate is green, the wasm build included.

## Docs to update on completion

- `docs/DATA-MODEL.md`: header's "Built" list; §Features and §The core
  feature types (`core.extrude` as built, `core.boolean`'s M1 form);
  §Bodies across features (ADR-0007); §The dependency DAG (the implicit
  edges, no longer "joins with join and cut"); §Evaluation (frozen
  states, the new codes); §Persistent naming (`mod` and `gen` as built,
  the split index); §Frozen results (ADR-0009, the name table's form);
  §File format ("What opens today": schema 2, blobs, the zip, the
  migration chain and its fixture).
- `docs/PLUGINS.md`: header; §One interface (the 0.3 world); §Versioning
  (0.3.0); the frozen states as a plugin author sees them.
- `docs/ARCHITECTURE.md`: header's "Built" list; §Crates (the zip crate,
  `arris` 0.4); §The kernel choke point (`fuse`, `cut`, `to_bytes`/
  `from_bytes` built, operands by content hash); the Arris asks table
  (A2 landed); §Testing (the slice scenario).
- `docs/CONCURRENCY-WASM.md` §Batch evaluation: `--without-plugin`,
  zips.
- `docs/ROADMAP.md`: M1's status line (landed, with ADR-0007 and
  ADR-0009), its in-list (primitives moved to the first feature needing
  them; `core.boolean`'s M1 form), the risk register (A2 retired), and the
  C1 Arris dependencies table.
- `AGENTS.md` current state: M1 landed, M2 next; §Commands (the
  `slice` scenario, `--without-plugin`).

## Open questions

- ⚠ OPEN 1, the human decides by step 7: **how the gear joins the
  plate.** The accept joins a plugin's body to a sketched one, and
  `core.extrude` can only join its own profile. Recommended: `core.
  boolean` (fuse and cut, one target, one tool) moves forward from M2 in
  that minimal form, and `gears.spur` stays as it is. It is on C1's
  in-list already, so no scope ADR is needed; M2 extends it to `common` and
  several tools. The alternative is a `mode` and a `target` on `gears.
  spur`. After step 4 it would be possible through the API alone, and it
  would prove a plugin modifying a body. But every plugin would then
  re-implement join, which is the core's job.
- ⚠ OPEN 2, the human confirms by step 9: **where frozen results are
  written** (ADR-0009 as proposed above). The alternative is a
  non-undoable `Freeze` command that the session issues after evaluation.
  It puts machine output into the command stream, moves generations on
  every evaluation, and makes "undo returns the earlier bytes" depend on
  the evaluator's timing.
- ⚠ OPEN 3, the agent decides in step 11: the zip crate (`zip` with
  `deflate` on `miniz_oxide`, or a minimal writer of our own). The crate
  must build for wasm32 with no C and give byte-identical output across
  runs and platforms. The choice goes in `docs/ARCHITECTURE.md` §Crates.
- ⚠ OPEN 4, the human, whenever: **Arris 0.4's release.** Steps 12–15
  wait on it, and releases are the human's. If this plan reaches step 12
  first, it stops there with steps 1–11 landed, and M1's status line
  names A2 as the one thing left.
- Found in step 5: **a body is read within its part.** `docs/DATA-MODEL.md`
  §The dependency DAG lets a feature take a body from another part as a
  tool. With versions, that reader would read the other part's modifiers,
  and an implicit edge across parts can close a cycle that `Document::
  validate` (registry-free) cannot see. Until assemblies, a body input or a
  `Topo` name into another part's body is refused `input.cross-part`, and
  every implicit edge stays inside a part. Recorded in §Bodies across
  features. No M1 scenario has two parts.
  Also: **a body input that nothing modifies is a tool** (ADR-0007), so a
  feature that only looks at a body must modify it and pass it on; the test
  reader does. `core.extrude`'s `target` input in cut and join modes
  modifies; `core.boolean`'s `tool` consumes.
- Found in step 4: **a feature that sweeps twice names both sweeps' caps
  alike** (`face:sweep.<feature>.end-cap` carries no curve key), so a
  boolean over both can give two entities one name, refused as
  `kernel.naming` (`crates/arrix-plugin-host/tests/tier0.rs` keeps to one
  boolean per feature). No M1 feature sweeps twice; `core.hole` and the
  patterns (M2) will. The fix is a naming change (a cap keyed, or a sweep
  index in the root), for M2's naming plan, not this one.
