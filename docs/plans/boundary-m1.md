# Plan: boundary-m1

- Started: 2026-10-09
- Cycle: the boundary opened, M1 (the plugin crosses); ADR-0016
- Idea: docs/ideas/the-boundary-opened.md (absorbed: its decisions 1C, 2C,
  3A and 4A, taken as recommended)

## Goal

A feature plugin written in Python, with ArriX's pure-stdlib SDK and no
Rust, is a first-class history entry: `arrix eval --plugin <dir>` starts it
as a Tier 2 child process, its `evaluate` builds geometry by calling back
into the host's kernel (recorded and named like a built-in's), a feature
downstream references its faces by name through an upstream edit, the
document saves with its frozen result and opens without the plugin with
the feature frozen and its reason given, and a crash or a timeout in the
plugin is that feature's diagnostic, never a hung evaluation. The wire is
JSON-RPC 2.0, newline-framed, with every shape generated from the WIT
world, chosen by an ADR that measured it. The risk retired: the plugin
world holds across a process boundary.

## Non-goals

- `arrix serve --stdio`, the wire `Rejected`, runtime `plugin.load` and
  `arrix-app render`: M2.
- `arrix test`, the harness's versioning and the cycle's accept: M3.
- 1C's read half (a body's bytes or mesh handed to a plugin for
  analysis): a WIT addition when an analysis scenario asks; backlog.
- A Tier 2 plugin returning bodies it built itself (1B's write half):
  refused, ADR-0021 says what would reopen it.
- Sandboxing a Tier 2 plugin's files or network; Tier 2 commands,
  importers, exporters or panels (features only); a directory watcher
  (4C); `pip` packaging of the SDK (the accept puts it on `PYTHONPATH`).
- The STEP unit (backlog): its own small plan, decided by the human.

## Design deltas

- **ADR-0021, Tier 2 builds by callback and is trusted, not sandboxed.** A
  Tier 2 plugin's geometry is made by the host's kernel through the
  `kernel` imports, called back over the wire during `evaluate`; bodies
  cross as `u32` handles scoped to one evaluation, never as bytes. The
  manifest's `kernel`, `document-read` and `document-write` capabilities are
  enforced at the wire; `files` and `network` are declared and shown, not
  enforced (it is the user's own Python). Amends `SEED.md` §8.4: Arris's
  Python binding is not needed by the boundary cycle, and the C1 blockers
  line is history. Commits `docs/ROADMAP.md` §The boundary opened with its
  milestones and amends its SDK line (no A10).
- **ADR-0022, the wire encoding**, closing the ⚠ OPEN in `docs/PLUGINS.md`
  (§Three tiers and §Open questions): JSON-RPC 2.0, one JSON object per
  line, both ways, reentrant (the plugin calls `kernel.*` while answering
  `feature.evaluate`); bulk numbers as base64 with an element type; a
  binary encoding only if the spike shows the pipe, not the kernel, is
  the cost. Records the spike's numbers.
- **The generated wire** (`crates/arrix-plugin-api/wire/`): from the WIT
  world, a JSON Schema per type and function, the method table, a Rust
  codec (`arrix-plugin-host`'s `wire` module) and the Python stubs
  (`python/arrix/_wire.py`), all held fresh by a test under
  `UPDATE_SNAPSHOTS=1`; `wit_equality`'s inventory covers them.
- **`arrix-plugin-host`**: `tier2` module (native only): spawn from
  `[tier2] command`, the `initialize` handshake (protocol version, the
  manifest's `api` range), `describe`, `evaluate` with kernel callbacks
  served on the evaluator's `&mut dyn Kernel`, a timeout, crash and
  restart; `register_tier2` adapts each type onto `arrix_doc::FeatureType`
  through the same path as Tier 0. New diagnostics `plugin.crash`,
  `plugin.timeout`, `plugin.protocol`, `plugin.capability`.
- **`arrix-plugin-api`**: no change expected (handles are scoped, cancel
  is the host's). If a step finds one, its commit states the semver.
- **`arrix-cli`**: `eval` and `describe` take `--plugin <dir>`
  (repeatable), a plugin directory holding an `arrix-plugin.toml`.
- **`python/arrix`**: the SDK, pure stdlib: types and stubs generated, a
  feature-type registration, a `kernel` proxy, the stdio loop
  (`python -m arrix.plugin <module>`); tested by `python -m unittest`, run
  from a cargo test so the gate covers it.
- **wasm-lint**: the thread table gains `crates/arrix-plugin-host/src/tier2`
  (native only, `cfg`-gated out of the wasm build).
- No document schema change: a Tier 2 feature's frozen result is stored as
  a Tier 0 one's is (ADR-0009).

## Arris dependencies

None new. Every kernel call is one `arrix-kernel` already makes for Tier 0
on the pinned `arris` 0.6 (extrude, revolve, names, face frame, measure,
fuse, cut, common, fillet, chamfer, copy, mirror). Arris's Python binding
(A10) is not used.

## Steps

- [x] **[2]** Step 1 — ADR-0021 (Tier 2 builds by callback, trusted not
  sandboxed, `SEED.md` §8.4 amended) and `docs/ROADMAP.md` §The boundary
  opened committed: status open, its SDK line amended, milestones M1 to
  M3 as in the idea's recommendation; the idea file deleted. Docs only.
- [ ] **[3]** Step 2 — The wire spike and ADR-0022: a `--nocapture` test
  measuring JSON against CBOR on the bracket's `meshed` events (size,
  encode, decode) and 1,000 `evaluate`-shaped round trips with ten kernel
  callbacks each over a real pipe to a child process; ADR-0022 written
  from its numbers, the ⚠ OPEN closed. CBOR stays a dev-dependency of
  the test, or goes with it.
- [ ] **[2]** Step 3 — The generator: from the parsed WIT world, the
  JSON Schemas and the method table under `crates/arrix-plugin-api/wire/`,
  held fresh by a test under `UPDATE_SNAPSHOTS=1`; `wit_equality`'s
  inventory fails on a WIT item the generator does not cover.
- [ ] **[2]** Step 4 — The Rust codec in `arrix-plugin-host`'s `wire`
  module, generated by step 3's generator: every plugin-API type to and
  from JSON, every case round-tripped and checked against its schema; the
  JSON-RPC framing (request, response, error, newline reader with a line
  limit) with a malformed-input test per error.
- [ ] **[3]** Step 5 — The Tier 2 host, happy path: spawn, `initialize`,
  `describe`, `evaluate` with kernel callbacks served on the host's
  kernel and handles scoped to the evaluation, `register_tier2`; tested
  against a fixture plugin written as a plain Python script in the
  crate's tests (no SDK yet): a feature extruded and filleted by callback,
  its call records and names equal to the same calls made by a Tier 0
  double. wasm-lint's thread table updated.
- [ ] **[2]** Step 6 — The Tier 2 host, failing soft: a crash, a timeout,
  a protocol error and a refused capability each the feature's diagnostic
  (`plugin.crash`, `plugin.timeout`, `plugin.protocol`,
  `plugin.capability`) with the evaluation carrying on; the process
  restarted on next use; the evaluator's interrupt honoured mid-evaluate;
  a stale handle refused. One test each, from the fixture script.
- [ ] **[2]** Step 7 — The Python SDK `python/arrix`: the generated stubs
  and types, `@feature_type` registration, the `kernel` proxy, the stdio
  loop; `python -m unittest` over a fake host, run by a cargo test in
  `arrix-plugin-host`; step 5's fixture rewritten on the SDK, its test
  unchanged.
- [ ] **[2]** Step 8 — `arrix eval --plugin <dir>` and `arrix describe
  --plugin <dir>`; a sample plugin `python/examples/standoff` (a
  hexagonal standoff on a plane input, height and hole parameters, built
  by extrude, cut and fillet) and `tests/docs/standoff` (a plate, the
  standoff on its top face, a fillet downstream naming one of its edges)
  with its eval golden; evaluated twice to identical bytes.
- [ ] **[2]** Step 9 — The frozen path and the record: the plate's width
  edited and followed with no reference lost; saved, opened without
  `--plugin`, the standoff frozen with its reason and the fillet still
  evaluating; stale after an edit, re-frozen on return; a kernel failure
  inside the Python feature leaving a replayable call record. This is the
  acceptance test.

## Acceptance

`cargo test -p arrix-cli --test boundary_m1` (`tests/docs/standoff`): the
Python standoff loaded by `--plugin`, evaluated through Tier 2, its edge
named downstream through an upstream edit, saved twice to identical
bytes, opened without the plugin (frozen, reason, downstream live, stale
and re-frozen), a crash and a timeout as diagnostics, a kernel failure
with its recipe; the whole gate green, the SDK's unittest included.

## Docs to update on completion

- `docs/PLUGINS.md` §Three tiers — Tier 2 as built (handles not bytes,
  the handshake, timeout and restart, `register_tier2`); §The manifest and
  §Capabilities — what Tier 2 enforces (ADR-0021); both ⚠ OPENs removed
  (ADR-0022); §The test kit — what M1 adds for a Python plugin.
- `docs/ARCHITECTURE.md` §Crates and §Gates — `python/arrix`, the
  generated `wire/`, the SDK's unittest in the gate; §Protocol boundary —
  the wire's framing, shared later by `serve`.
- `docs/CONCURRENCY-WASM.md` §Roles and threads — the Tier 2 reader
  thread, native only.
- `docs/adr/README.md` — ADR-0021 and ADR-0022 indexed.
- `docs/BACKLOG.md` — 1C's read half; anything the steps defer.
- `docs/ROADMAP.md` status line — M1 done, its date.
- `AGENTS.md` current state and §Commands — `--plugin`, the SDK's tests,
  `--test boundary_m1`.

## Open questions

- ⚠ OPEN: whether a Tier 2 evaluation needs any plugin-API change (agent,
  by step 5). Expected none; a change is stated with its semver in that
  commit.
- ⚠ OPEN: the Tier 2 timeout's default and where it is set (agent, by
  step 6): a manifest field, a setting, or both, listed by `describe`.
