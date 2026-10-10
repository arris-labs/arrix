# Plan: boundary-m3

- Started: 2026-10-10
- Cycle: the boundary opened, M3 (the kit and the accept); ADR-0016
- Idea: docs/ideas/boundary-m3-kit-and-harness.md (absorbed: 1A, 2A, 3A
  and 4A as recommended; `arrix-plugin-test` deferred to C3; the forms
  checked by spec, not by a render; `debug_state()` promised only on its
  marked fields)

## Goal

An agent with nothing but the installed `arrix` and `arrix-app` binaries
and the Python SDK can do all of ADR-0016's scenario. It writes a feature
plugin and drives `arrix serve --stdio` through `arrix.session`. It builds
and saves a document, renders it headless and checks the render's JSON
against a published schema. It tests the plugin with `arrix test`, and it
sees the document open frozen without the plugin. The harness is now a
public interface, as ADR-0026 states.
- **Versions.** The eval line, the render JSON, the kit's report and the
  session protocol each carry a version.
- **Schemas.** Each surface prints its JSON Schema, and a golden test
  keeps each one fresh.
- **Policy.** Changes inside a version are additive only.
- **`debug_state()`.** It is promised on the fields its schema marks.

`arrix test` runs the same checks on any plugin directory, whatever its
tier: determinism, round trip, frozen open, migration coverage, spec
validation, and an expected kernel failure's recipe. That includes
`plugins/gears`.

## Non-goals

- `arrix-plugin-test`, the Rust kit for Tier 0 authors. It moves to C3,
  beside the Tier 1 SDK.
- `migrate` in the WIT. No feature type has a second `type_version`, so
  the migration check covers fixtures only (`docs/PLUGINS.md` §1).
- Rendering forms in the kit. ADR-0018 makes a valid spec a renderable
  form, and `arrix` never links wgpu (ADR-0025).
- Typed Python command classes generated from the protocol schema.
  Commands stay plain dicts (backlog).
- A `pyproject.toml` or pip install exercised by a test, and
  `cargo install` inside a test.
- Unloading a plugin, transports other than stdio, several clients, and
  remote mode (the cycle's "out").
- Any document schema change.

## Design deltas

- **ADR-0026, the harness is versioned.** It names the surfaces, each with
  its own version and how a reader finds it:
  - the eval line, by its `format`;
  - the render JSON, by its `format`;
  - the kit's report, by its `format`;
  - the session protocol, by `protocol` in `initialize`.

  It also states:
  - **Schemas**: each surface's schema is printed by its `--schema` and
    held by a golden test.
  - **Additive-only changes inside a version**: a new optional field, or
    a new enum value that readers must tolerate. A removal, a rename or a
    change of meaning bumps the version, and the commit body says so
    (`Harness: <surface> format N, …`), like the API's semver line.
  - **The promised part of `debug_state()`** is the fields its schema
    marks (`x-arrix-promised`): bodies, camera, badges, evaluation and
    diagnostics. Everything else is informational.
  - **The render JSON's shape** `{format, adapter, debug_state}`, which
    ADR-0025 left for M3.
- **`arrix-cli`**:
  - `eval` lines gain `"format": 1`, and `arrix eval --schema` prints the
    line's schema.
  - A new `arrix test <plugin-dir> [--plugin <dir>]… [--schema]` writes
    one JSON report line per check, and exits 1 on any failure.
- **`arrix-plugin-host`**: the manifest gains a `[test]` table:
  `documents = [<paths relative to the plugin>]`, and `fail = [<doc>]` for
  fixtures expected to fail with a kernel category. An unknown key is
  still a diagnostic. `plugins/gears` and `python/examples/standoff` each
  get fixtures.
- **`arrix-app`**: `arrix-app render --schema` prints the render JSON's
  schema. `DebugState` gets a schema with the promised fields marked
  (derived or hand-written, agent's call at step 3).
- **`arrix-doc`**: a subscriber that joins after `Opened` hears the
  latest generation's outcomes again: its `Evaluated`, `Finished` and
  refined `Meshed`. This is additive, and the protocol stays at 1. The
  schema golden is refreshed if any type's shape changes.
- **The Python SDK `python/arrix`**:
  - `arrix.session`: `Session.spawn(arrix=…, plugins=…, doc=…)`,
    `initialize`, `submit`, `save`, `load_plugin`, `reload_plugin`,
    `export`, an event iterator and `settle(generation)`. It is pure
    stdlib.
  - `arrix.eval(paths, …)` returns the parsed eval lines.
- **No change to `arrix-plugin-api`.** It stays at 0.5.0, and the WIT is
  unchanged.

## Arris dependencies

None new. The kit, the client and the accept reach the kernel only through
calls `arrix-kernel` already makes on the pinned `arris` 0.6. The kernel
failure check reuses `--save-fixtures` recipes (`SEED.md` §6.6).

## Steps

- [x] **[2]** Step 1: ADR-0026 is written and indexed, and the plan file
  is committed with it (the idea was absorbed and never committed). The backlog line on `Opened` is removed, now that step 4
  takes it. Docs only.
- [x] **[2]** Step 2: the eval format. Every eval line carries
  `"format": 1`, and the eval goldens in `crates/arrix-cli/tests/golden/`
  are refreshed for that field alone. `arrix eval --schema` prints the
  line's schema, held as `eval.schema.json` under `UPDATE_SNAPSHOTS=1`.
  A test validates every eval golden, a sweep and a `--timings` line
  against it.
- [x] **[2]** Step 3: the render's schema. `arrix-app render --schema`
  prints `{format, adapter, debug_state}`, with `debug_state()`'s
  promised fields marked `x-arrix-promised` and the rest open
  (`additionalProperties` allowed). It is held as a golden in
  `arrix-app`'s tests. `render_cli` validates every render it makes
  against the schema. A test checks that every marked field is present in
  a slice render's JSON.
- [x] **[2]** Step 4: `Opened` followed by the latest outcomes. A client
  of `serve tests/docs/standoff` hears generation 0's `Evaluated`,
  `Finished` and refined `Meshed` with no change made first. Tested in
  `arrix-doc` with a late subscriber, and on the spawned binary in
  `serve.rs`. The workaround in `boundary_m2.rs` and its comment go.
- [x] **[2]** Step 5: the manifest's `[test]` table, and `arrix test`'s
  core: determinism and round trip.
  - Fixtures: `plugins/gears` gets `tests/docs/gear-on-plane` by a
    relative path or a copy (agent's call), and the standoff example gets
    `tests/docs/standoff`'s.
  - Determinism: two evaluations, each on a fresh kernel and a fresh
    Tier 2 process, save to identical bytes.
  - Round trip: save, open, save gives identical bytes and an identical
    eval line.
  - Report: one JSON line per check, `{format, plugin, check, doc,
    status, detail}`, plus `--schema` and its golden.
  - Tests: gears and the standoff pass. A planted non-deterministic
    Python plugin (a random height) fails determinism with its detail.
    A missing fixture and a bad `[test]` key are each a diagnostic with
    the manifest's file and line.
- [x] **[2]** Step 6: the rest of the checks.
  - Frozen: open without the plugin. Every one of its features is frozen
    with a reason, each with a stored result keeps its bodies, and
    downstream features evaluate.
  - Migration: every `type_version` the plugin's specs declare has a
    fixture holding it, and the fixture evaluates.
  - Spec: every spec validates (ADR-0018).
  - Kernel failure: a `fail` fixture fails with a `kernel.*` category and
    leaves a recipe that replays.

  Each check gets a passing case on gears or the standoff and a planted
  failing one: a plugin whose fixture lacks a type, and a `fail` fixture
  that succeeds.
- [x] **[2]** Step 7: `arrix.session` and `arrix.eval` in the SDK, with
  `python -m unittest` cases run by `python_sdk.rs` against the built
  `arrix`.
  - The session spawns, initializes, submits the plate as a dict command,
    settles and saves.
  - A stale submit's `err` arrives as `arrix.session.Rejected`, and a
    protocol error as `ProtocolError`.
  - `load_plugin` and `reload_plugin` work.
  - `serve` exiting is reported, never a hang (a timeout on every wait).
  - `arrix.eval` of `tests/docs/standoff` parses the line and checks its
    `format`.
- [ ] **[3]** Step 8: the cycle's accept, `cargo test -p arrix-cli --test
  boundary_m3`. The test copies the built `arrix`, `arrix-app` and
  `python/arrix` into a temp dir, then runs
  `crates/arrix-cli/tests/boundary/agent.py` with a cleared environment:
  `PATH` has only the copies and the directory of the system `python3`,
  `PYTHONPATH` has only the copied SDK, and the working directory is the
  temp dir.

  The script, acting as the agent:
  1. writes a new plugin (`boss`, a boss on a face; not the standoff)
     with its manifest and a `[test]` fixture of its own;
  2. loads it into `serve` through `arrix.session`, builds a plate and a
     boss on its top face, settles, and saves;
  3. renders with `arrix-app render --json` and validates the render
     against `arrix-app render --schema`, using a minimal checker in the
     script, stdlib only; the promised `bodies` must show two bodies and
     the boss's bounds;
  4. runs `arrix test` on its plugin and reads every line as `pass`;
  5. runs `arrix eval --without-plugin boss` on the saved document and
     sees the boss frozen with its reason and the plate whole.

  The render step skips loudly without lavapipe, and fails under
  `ARRIX_REQUIRE_GPU=1`. The test also asserts that the script never
  touched the workspace, by checking that the workspace path appears in
  none of its arguments or environment.

## Acceptance

`cargo test -p arrix-cli --test boundary_m3` is ADR-0016's scenario: an
agent with only the installed binaries and the SDK writes, loads, builds,
renders, checks and tests a Python plugin, and sees its document frozen
without it. The schema goldens of the four surfaces (`eval.schema.json`,
the render's, the kit's and `protocol.schema.json`), `--test boundary_m1`
and `--test boundary_m2` stay green, as does the whole gate (pre-commit
and pre-push).

## Docs to update on completion

- `docs/PLUGINS.md` §The test kit: `arrix test` as built (the checks,
  the report, the `[test]` table); `arrix-plugin-test` moved to C3; the
  forms checked by spec. §The manifest: the `[test]` table.
- `docs/CONCURRENCY-WASM.md` §Batch evaluation: the line's `format` and
  `--schema`, and `arrix.eval` as "the same through the Python API".
- `docs/UI-RENDERING.md` §Visual debugging: the render's schema, the
  promised `debug_state()` fields, ADR-0026.
- `docs/ARCHITECTURE.md` §The protocol boundary (`Opened` followed by
  outcomes; `arrix.session`), §Gates (the new schema goldens, like
  `serve_schema`).
- `docs/BACKLOG.md`: typed Python command classes;
  `arrix-plugin-test` under C3 if the roadmap's C3 section doesn't take it.
- `docs/ROADMAP.md`: the cycle's status line marks M3 done and the cycle's
  accept met. C3's in-list gains `arrix-plugin-test`. Closing the cycle
  is `/close-cycle`'s job.
- `AGENTS.md` current state: M3 done. Commands: `arrix test`,
  `eval --schema`, `render --schema`, `arrix.session`, `--test
  boundary_m3`.

## Open questions

- ⚠ OPEN: is the manifest given a JSON Schema in `config schema` (as
  `plugin`) now that it grows a table? Constraint 3 leans yes. Decided at step 5:
  it costs more than the step, so it goes to the backlog.
- ⚠ OPEN: does gears get its fixture by a path outside its directory, or
  by a copy inside `plugins/gears/tests/`? A copy keeps a plugin
  self-contained, as an outside plugin would be. Decided at step 5: a copy,
  in `fixtures/` (`plugins/gears/fixtures/gear-on-plane`,
  `python/examples/standoff/fixtures/standoff`).
- ⚠ OPEN: is `DebugState`'s schema derived (`schemars` in `arrix-app`) or
  hand-written over the promised fields only? Decided at step 3: hand-written
  (`render::schema()`), promised fields only, the rest open.
