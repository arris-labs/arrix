# Architecture

The crates, the rules between them, the one data flow, where threads run,
the kernel's choke point, errors, testing and the gate. The charter is
`SEED.md` §6; the document model is `docs/DATA-MODEL.md`, the plugin model
`docs/PLUGINS.md`, the UI `docs/UI-RENDERING.md`, threads and the browser
`docs/CONCURRENCY-WASM.md`. Built: the workspace with every crate
(`arrix-core`'s units, ids, references, persistent names, frames and
profiles, and `Diagnostic`, `arrix-kernel`'s first slice of §The kernel
choke point, `arrix-doc`'s empty-document
open and `arrix eval` line, the empty shell in `arrix-ui` and `arrix-app`,
the rest stubs), §Errors and diagnostics as far as `Diagnostic` goes, the
UI and CLI rows of §Testing, and §Gates. Each other section becomes true as
C1 lands, and the commit that builds it keeps it true.

## Crates and the layer rule

One Cargo workspace, edition 2024, resolver 3, stable toolchain with the
`wasm32-unknown-unknown` target. Every crate is `#![forbid(unsafe_code)]`
unless an ADR says otherwise.

| Crate | Holds | May depend on (workspace) | Notable external |
|---|---|---|---|
| `arrix-core` | units and quantities, ids, plugin ids, plain math over `glam` (f64), persistent-reference types, `Diagnostic` | — | `glam`, `serde`, `thiserror` |
| `arrix-kernel` | the only crate naming Arris types: operations, naming from provenance, render meshes, body bytes, call records | core | `arris`, `serde`, `thiserror` |
| `arrix-sketch` | the sketch model, the constraint solver, inference, trim/extend/offset/mirror (ported) | core | — |
| `arrix-plugin-api` | the WIT world, its Rust traits and plain types; semver of its own | core | `wit-bindgen` (types only) |
| `arrix-doc` | document, parameters and expressions, DAG, feature registry, commands, undo, evaluator, `.arrx`, the `arrix eval` line; reads no files (a `DocumentSource` hands it bytes) | core, kernel, sketch, plugin-api | `serde_json`, `zip`, `blake3` |
| `arrix-plugin-host` | tier 0/1/2 hosting, manifests, capabilities, the frozen fallback | core, doc, plugin-api | `wasmtime` (native, C3) |
| `arrix-viewport` | wgpu scene, ID-buffer picking, overlays, camera (ported) | core | `wgpu`, `egui-wgpu` |
| `arrix-ui` | egui widgets over plain view types; the declarative-UI renderer | core, viewport, plugin-api (UI types only) | `egui` |
| `arrix-app` | the binary: the shell, the command registry, translation between document and view types | all of the above | `eframe` |
| `arrix-cli` | headless open / eval / export / run / test | core, doc, plugin-host, kernel | `clap` |
| `plugins/*` | first-party plugins: `gears` (C1), `robotics` (by C3) | plugin-api **only** | — |
| `python/arrix` | the Tier 2 SDK (C3), not a Cargo crate | — | — |

Crates live in `crates/<name>/`, plugins in `plugins/<name>/` (package
`arrix-<name>`: `plugins/gears` is `arrix-gears`). External versions are
pinned once, in the root `Cargo.toml`'s `[workspace.dependencies]`:
`arris` 0.2, and one egui minor for `egui`, `eframe`, `egui-wgpu` and the
dev-only `egui_kittest` (0.36). The table's externals not yet in a
manifest (`wit-bindgen`, `zip`, `blake3`, `wgpu` directly, `wasmtime`)
join with the code that needs them; `glam` (0.33, f64 types, `serde`)
has. `unsafe_code = "forbid"` is a workspace lint every crate inherits.
The binaries are `arrix` (`arrix-cli`) and `arrix-app`. No crate is
published yet (`publish = false`); reserving names is the human's act.

**The layer rules** (`SEED.md` §6.7) are binding and checked by the layer
lint (§Gates):

1. Arris types appear only in `arrix-kernel`. Every other crate sees
   ArriX's own types: `KernelBody` handles, `RenderMesh`, `PersistentName`.
2. `egui`, `eframe` and `wgpu` appear only in `arrix-viewport`, `arrix-ui`
   and `arrix-app`.
3. `arrix-ui` never names a document or sketch type. It renders view types
   (`TreeView`, `FormView`, `RibbonView`, …) and returns `UiIntent`s;
   `arrix-app` translates both ways.
4. A plugin depends on `arrix-plugin-api` alone, first-party ones
   included.
5. Mutation only through `Command`s (`docs/DATA-MODEL.md` §Commands and
   undo). No crate outside `arrix-doc` holds a `&mut Document`.
6. The kernel never runs on the UI thread. `arrix-app` never calls
   `arrix-kernel`; it talks to the evaluator. The sketch solver is the one
   sanctioned exception (`SEED.md` §6.1).

The dependency graph enforces most of these by construction; the lint
catches the rest (a UI crate that pulls a doc type through a re-export, a
plugin with a second `arrix-*` dependency, a new crate that takes `arris`
directly).

## The one data flow

```
 UI input ──► UiIntent ──► arrix-app ──► CommandEnvelope ──► Session
                                                               │
                             ┌─────────── document applies ◄───┘
                             │  (new generation, inverse on the author's undo stack)
                             ▼
                  evaluator (worker) ── replays what changed ──► arrix-kernel ──► Arris
                             │
                             ▼
                        EvalEvents ──► Session subscribers ──► arrix-app ──► view types ──► arrix-ui
```

A plugin sits on the same path: it issues commands and receives events,
never touching the document or the scene directly (`SEED.md` §6.1).

### The protocol boundary

The line between a client (UI, CLI, a plugin's command side) and the
document is a protocol even in one process, so that a self-hosted server
with thin clients stays possible (`SEED.md` §6.1). Four rules hold from
day 0:

1. **Commands, `EvalEvent`s and view types are plain, serialisable,
   versioned data** (`serde`), with no `Rc`, closures, kernel handles or
   pointers into the document. Meshes cross with their persistent names,
   so picking stays client-side.
2. **Undo is per author, by inverse commands.**
3. **A command names its base generation**; the document applies it,
   or rejects it as stale.
4. **Session state belongs to the client**: selection, the active sketch,
   the camera, the edit mode.

The boundary is the `Session` trait in `arrix-doc`: `submit(envelope) ->
Result<Applied, Rejected>` and `subscribe() -> EventStream`, whose events
are the applied commands (from which each client keeps its own replica of
the document, `docs/CONCURRENCY-WASM.md` §Roles and threads) and the
`EvalEvent`s. Local mode is `LocalSession`, an in-process transport over
channels, and it is the only one that exists. A round-trip test
serialises every command and event through JSON and back on every run, so
nothing unserialisable can creep across. No network code exists until
remote mode is built.

## Threads

| Thread | Role | Runs | Never runs |
|---|---|---|---|
| UI (main) | client | egui, the viewport's frame, input, the sketch solver during sketch edits, the document replica, command submission | kernel calls, file I/O beyond a picked file's bytes |
| Session | authority | the document: applies or rejects commands, publishes the applied stream, hands snapshots to the evaluator | kernel calls, anything egui |
| Evaluator | authority | evaluates snapshots, calls the kernel, tessellates | anything egui |
| Plugin processes | — | Tier 2 plugins, one process each (C3) | — |

The executor, cancellation, the wasm story and budgets are in
`docs/CONCURRENCY-WASM.md`.

## The kernel choke point

`arrix-kernel` is the only door to Arris (`SEED.md` §6.6). It exists for
three reasons: to keep Arris's breaking enums out of the plugin API, to name
geometry from provenance, and to make every kernel call a record.

- **`Kernel` service.** A per-evaluation context owning one Arris `Model`
  (or a model per independent branch, `docs/CONCURRENCY-WASM.md`), exposing
  ArriX-signature operations: `extrude`, `revolve`, `boolean`, `fillet`,
  `chamfer`, `transform`, `mirror`, primitives, `project_to_plane`,
  `face_frame`, `mass_properties`, `tessellate`, `export_step`,
  `export_stl`, `export_obj`, `to_bytes`/`from_bytes` (ask A2). Handles
  it returns are opaque (`KernelBody`) and valid within the evaluation.
  Built: `extrude` (a keyed `Profile` along its plane's normal, a
  negative distance against it), `face_frame` (outward, planar faces
  only) and `mass_properties` (volume, area, centroid).
- **Units.** The model's `Precision` is set for metres at the micrometre
  scale, matching SI inside: `default_tolerance` 1e-6, Arris's defaults
  otherwise, as Arris's own `probe-*-m` fixtures carry.
- **Naming.** Each operation returns its outputs with their
  `PersistentName`s, derived from Arris's `Provenance` in this crate
  (`docs/DATA-MODEL.md` §Persistent naming). Sketch entity ids map to
  Arris's `(loop_index, segment)` here and nowhere else. Every face, edge
  and vertex of a result has exactly one name and every name one entity
  (`BodyNames`); a result that breaks either, or carries an origin the
  operation should not record, is refused as `kernel.naming` with its
  record kept, never named by position.
- **Call records.** Before an operation or query runs, it is described
  by a value: `KernelCall { op, operands, precision, arris_version }`,
  `op` carrying its arguments in ArriX's own types (`KernelOp::Extrude {
  feature, profile, distance }`), `precision` the recipe's
  `default_tolerance`, `arris_version` the locked release (a test holds it
  to `Cargo.lock`). Operands are bodies by content hash once body bytes
  land (ask A2); until then an operand names the call that made it, by
  its `CallIndex`. The `Kernel` keeps every record of its evaluation,
  failed calls included, and a failure names its record's index. On
  failure the failing record, plus its operands' body bytes, is kept
  beside the diagnostic.
- **Fixture export.** A kept record serialises as a self-contained Arris
  fixture recipe (Arris's `fixture.json`): the operands' closures as body
  bytes, the operation and its arguments, the precision, and nothing else
  of the document. "Save as kernel fixture" is a command in the app and
  `--save-fixtures <dir>` in `arrix eval`. Until body bytes land (ask A2),
  a record whose operands are primitives or sweeps of profiles serialises
  in Arris's existing recipe vocabulary; one that needs a body operand
  names A2 as its blocker rather than approximating.
- **Failure categories.** Every failure maps to a stable, countable key:
  the kernel's typed error with its reason (`OpError::Degenerate(
  TangentContact)`), the feature type, and the plugin id when a plugin
  called it. The kernel's part is a `DiagnosticCode`,
  `kernel.<error>[.<reason>]` in lower-kebab words
  (`kernel.degenerate.not-positive`, `kernel.internal.checker`,
  `kernel.profile`), and `kernel.naming`, `kernel.unknown-body`,
  `kernel.unknown-name`, `kernel.wrong-kind` for ArriX's own refusals. The same failure across a thousand documents is one row with
  a count in `arrix eval`'s summary.
- **Nothing leaves the machine unasked.** No code in this crate, or any
  crate, sends a record anywhere. Reporting upstream is a backlog item
  (`docs/BACKLOG.md`), opt-in per report, shrunk and previewed, and its
  network code will live in one place, off by default.

A kernel gap is never worked around here: no tolerance nudge, no retry, no
geometric matching. It becomes an Arris fixture or an ask (§Arris
dependencies).

## Errors and diagnostics

- **Library errors** are `thiserror` enums per crate, naming the entities
  involved (which feature, which reference, which face pair). No `anyhow`
  outside `arrix-app` and `arrix-cli`.
- **User-facing failures are `Diagnostic`s** (`arrix-core`): severity
  (`info`, `warning`, `error`), a stable code (`ref.lost`,
  `kernel.degenerate.tangent-contact`, `plugin.frozen.missing`), a
  message, the persistent references to highlight, and for a lost
  reference the ranked candidates. The code's grammar is
  `segment(.segment)*`, each segment lower-case letters and digits in
  hyphen-joined words starting with a letter, checked when a
  `DiagnosticCode` is built or deserialised. `refs` and `candidates` are
  `Ref`s, the candidates ranked best first and omitted from JSON when
  empty; the core never applies a candidate. A diagnostic is data: it crosses the protocol boundary and
  the plugin boundary, and `arrix eval` prints it as JSON.
- **Fail soft, never silently** (`SEED.md` §8.2): a failed feature keeps
  its parameters and says why; the rest of the model lives. No panic on
  user input; a panic in a plugin's Tier 0 code is caught at the host
  boundary and becomes that feature's diagnostic.

## Testing

Everything is checkable without a human looking at a screen (`SEED.md`
§1).

| Layer | How |
|---|---|
| Unit and property | in each crate; `proptest` for expressions, the DAG, naming and the file format's determinism |
| Sketch solver | the ported scenario suite: build a sketch, solve, assert geometry, DoF and diagnostics |
| Document | command → state → inverse round trips; undo/redo gives byte-identical files; stale-command rejection |
| Evaluation | scenario documents in `tests/docs/<name>/` (the unzipped form, so a diff reads), evaluated by the same code path `arrix eval` uses, asserting volume, area, counts, names and diagnostics |
| Naming | an edit that keeps a face keeps its name; a lost reference is a diagnostic with candidates, never a rebind |
| Plugins | the in-tree test kit: determinism (evaluate twice), round trip, frozen open, migration (`docs/PLUGINS.md` §The test kit) |
| Protocol | every command and event serialised through JSON and back |
| UI | headless `egui_kittest` snapshots on wgpu, each a text golden: the `debug_state()` JSON and a coarse frame; images rendered on demand, never committed (`docs/UI-RENDERING.md` §Visual debugging, ADR-0001) |
| CLI | golden JSON of `arrix eval` over the scenario documents: the binary run from the workspace root by `crates/arrix-cli/tests/eval.rs`, compared byte for byte with `crates/arrix-cli/tests/golden/<name>.jsonl` |

A wanted behaviour that does not work yet is not an `#[ignore]`d test. If
the gap is the kernel's, it is an Arris fixture or ask; if it is ours, it
is a plan step.

## Gates

`.githooks/` holds the gate and CI mirrors it (`.agents/rules/git.md`);
`git config core.hooksPath .githooks` once per clone or worktree.
`AGENTS.md` §Commands repeats how to run each part. The lints are Python 3
scripts sharing `scripts/lintlib.py`: they read Rust source as written,
with comments and literal contents blanked first, and each takes the
repository root from its own path. CI, `.github/workflows/ci.yml`, runs
the same commands in five jobs (fmt, clippy, lints, test, wasm) on a push
to `main` and on pull requests; its steps named `gate: …` are exactly the
hooks' commands, which `gate-selftest` checks. The test job installs
`mesa-vulkan-drivers` and sets `ARRIX_REQUIRE_GPU=1`; when it fails, it
runs `scripts/snapshot-baseline HEAD~1` and uploads `target/snapshots/`
as an artifact.

**pre-commit** (fast):

1. `cargo fmt --all -- --check`
2. `cargo clippy --workspace --all-targets --all-features -- -D warnings`
3. `scripts/size-lint`: at most 1,500 lines per file and 200 per function
   in `crates/` and `plugins/` (`SEED.md` §8.2). Integration tests are
   skipped; an inline `mod tests` and a `tests.rs` file are exempt from the
   function limit. The allowlist, `scripts/size-allowlist.txt`, is empty;
   its growth is the signal to split, and every entry names the plan that
   removes it and has the human's OK.
4. `scripts/wasm-lint`: what compiles for wasm32 and fails there. No
   `std::time::Instant` or `SystemTime` (use `web_time`; a deliberately
   native-only item carries `#[allow(clippy::disallowed_types, reason =
   …)]`); no `thread::spawn` or `thread::Builder` outside the executor
   (`crates/arrix-doc/src/executor`); no `std::fs` outside `arrix-app`'s
   native file dialog (`crates/arrix-app/src/file_dialog`) and
   `arrix-plugin-host`. `arrix-cli` is native-only and exempt from the
   thread and fs rules, as is test code. The permitted places are one
   table at the top of the script.
5. `scripts/layer-lint`: the layer rules above, two ways. **Edges**:
   every package's declared dependencies (`cargo metadata --no-deps`, so a
   rename is seen through; dev and build dependencies included) against
   the allowed-edges table of §Crates, kept in the script; a crate the
   table does not name fails until it is added; `arris*` only in
   `arrix-kernel`, `egui*`/`eframe`/`wgpu*` only in viewport, ui and app;
   and `arrix-ui` must not reach `arrix-doc` or `arrix-sketch`
   transitively, so a re-export cannot carry a doc type to it. **Paths**:
   source as written, no `arris::` outside `arrix-kernel`, no egui, eframe
   or wgpu path outside the UI crates, no `arrix_doc` or `arrix_sketch` in
   `arrix-ui`, no `arrix_*` but `arrix_plugin_api` in `plugins/`.

**pre-push** (slow):

1. `cargo test --workspace`
2. `cargo test --workspace --all-features`
3. `cargo build --target wasm32-unknown-unknown --workspace --exclude
   arrix-cli`. `arrix-plugin-host`'s native tiers (C3) go behind a
   target cfg so the crate still builds.
4. `scripts/gate-selftest`: each lint run on a copy of the tree with a
   deliberate violation planted (an oversize function, `Instant::now` in a
   core crate, `std::fs` in `arrix-doc`, a thread spawned in
   `arrix-kernel`, `egui` in `arrix-doc` as a dependency and as a path,
   `arris` outside the kernel, a second `arrix-*` dependency in
   `plugins/gears`, `arrix-ui` reaching `arrix-doc` through another
   crate), which must fail it, and with the exemptions each lint promises
   (test code, the audit marker, a comment), which must pass. It also
   checks that CI's `gate: …` steps run exactly the hooks' commands, each
   as often, and that the check fails on a planted drift of either side.

The visual tests need a CPU wgpu adapter, Mesa's lavapipe (CI installs
`mesa-vulkan-drivers`). With none a scenario skips loudly, never silently
passes, and with `ARRIX_REQUIRE_GPU=1`, as in CI, it fails
(`docs/UI-RENDERING.md` §Visual debugging).

## Arris dependencies

Arris is a crates.io dependency with a pinned minor, and `[patch.crates-io]`
only for an unreleased fix (`SEED.md` §8.3). What ArriX asks of it is
recorded in Arris's repository (`docs/ideas/arrix-consumer-asks.md` there).
A step blocked on an ask names it and waits.

| Ask | What | Needed by | State |
|---|---|---|---|
| A1 | consumer roles: `Role::Consumer { namespace, key }` | C1: plugin topology built directly | asked |
| A2 | body bytes with a compatibility policy | C1: frozen results, fixture operands; C3: the Tier 2 boundary | asked |
| A3 | cancellation: an interrupt token, optionally a step budget | C1: superseded evaluations; C4: the browser | asked |
| A11 | `ops::mirror` | C1: the mirror feature | asked |
| A5 | `region2` as public API | C1–C2: sketch regions | asked |
| A4 | STEP product structure | C2 | asked |
| A8, A9, A6, A7, A10 | multi-tool booleans, per-face tessellation, the query cycle, the sweep and healing cycles, the Python binding | C2 and later | asked |

This table is brought up to date at every cycle close (`/close-cycle`).

## Open questions

None of its own. The two open at kickoff are in `docs/DATA-MODEL.md`
(undo granularity for plugin commands) and `docs/PLUGINS.md` (the Tier 2
wire encoding).
