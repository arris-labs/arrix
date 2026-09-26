# Concurrency and wasm

Where work runs, how an edit supersedes an evaluation, what determinism
promises, how many documents are evaluated at once, what the browser build
is, and the performance budgets. Built: `arrix eval` on one document
directory (§Batch evaluation) and the evaluator's synchronous core with
its cache (§The evaluator). Each other section becomes true as its cycle lands (the native executor and batch evaluation
in C1, the browser in C4), and the commit that builds it keeps it true.

## Roles and threads

Three roles, named after the server-and-thin-client split they must not
preclude (`SEED.md` §6.1). Locally all three are in one process.

| Role | Thread (native) | Does |
|---|---|---|
| **Client** | the UI thread (or the CLI's main thread) | input, egui, the viewport, the sketch solver during sketch edits; keeps a **replica** of the document; submits commands |
| **Authority** | the session thread | holds the document; applies or rejects each command; publishes the applied stream; hands snapshots to the evaluator |
| **Evaluator** | one worker thread (C1) | evaluates snapshots through `arrix-kernel`, tessellates, publishes `EvalEvent`s |

- **Replicas.** A client does not read the authority's document. It
  applies the same applied-command stream to its own copy, which is cheap
  (applying is pure and fast) and makes the protocol boundary real on every
  run. A debug build checks the replica's hash against the authority's at
  each generation.
- **Snapshots** are immutable and cheap to clone: the document is a tree
  of `Arc`ed parts and records, so a snapshot shares everything a command
  did not touch.
- Tier 2 plugins are separate processes (C3); the evaluator waits on them
  like on any other feature.

## The evaluator

- **Newest generation wins.** When a command is applied during an
  evaluation, the evaluator finishes or cancels its current feature, then
  continues from the new snapshot. Outputs whose input hash did not change
  are reused from the cache, so continuing is cheap.
- **Cancellation.** A feature whose inputs changed is cancelled, not
  finished: an interrupt token passed to the kernel, which stops at a loop
  boundary and rolls its transaction back (Arris ask A3). Until A3 lands,
  cancellation is checked between kernel calls only, and one stale
  operation may run to completion; its result is cached if its hash is
  still wanted and dropped if not. No thread is ever killed.
- **Events carry their generation.** A client drops an event older than
  the latest generation it has drawn for that feature.
- **The cache** maps input hash to outputs (`docs/DATA-MODEL.md`
  §Evaluation), bounded in bytes, least recently used first out, shared
  across generations. Outputs are Arris bodies in one long-lived model per
  evaluator; after each completed evaluation, `Model::retain` drops the
  bodies no cache entry holds.
  Built: the cache and the long-lived kernel (`arrix_doc::Evaluator`),
  bounded by entry count (`DEFAULT_CAPACITY`, 256) until a body's size is
  known (body bytes, ask A2); an entry the latest evaluation used is never
  evicted. The kernel's call records outlive the bodies they made, so a
  later failure's operands still name them.
- **Order.** C1 evaluates features one at a time in topological order.
  Parallel evaluation of independent branches (parts, patterns) is
  clone-evaluate-import across Arris models, which needs body bytes (ask
  A2) and a measurement that says it pays; it is not built before both.
- **Tessellation** runs on the evaluator after each body: a coarse mesh
  first, then a refined one, each its own event.

## Determinism

Evaluating the same snapshot gives byte-identical bodies, names, meshes
and saved files, on any machine, native or wasm (`SEED.md` §6.3, §6.6).
Replay, memoisation, frozen results, fixture records and golden tests all
stand on it.

- No clock, randomness or environment reads in `arrix-core`,
  `arrix-kernel`, `arrix-sketch` or `arrix-doc` evaluation paths. Ids are
  minted by the client when it builds a command, never during evaluation.
- No `HashMap` or `HashSet` iteration reaches an output; ordered maps or
  sorted collections only.
- Floating point: no fast-math, no platform intrinsics whose result
  differs by target. Arris's `parallel` feature is byte-identical by its
  own contract; the tests run both ways.
- A plugin's `evaluate` has the same obligation, checked by the test kit
  (`docs/PLUGINS.md` §The test kit).
- **Time budgets are not determinism.** A wall-clock timeout makes a
  result depend on the machine. Batch evaluation uses Arris's step budget
  (ask A3) where it exists; a wall-clock timeout is a last resort, and a
  result it cut short is reported as `timeout`, never as a kernel failure.

## Batch evaluation

Batch evaluation is a first-class API, not a test helper (`SEED.md` §6.6):
`arrix eval` in C1, the same through the Python API in C3. Dataset
replay, agent users and continuity sweeps are built on it.

```
arrix eval <docs or dirs>… [--sweep <feature>.<param>=<from>..<to>:<n>]
           [--without-plugin <id>]… [--save-fixtures <dir>]
           [--budget <kernel steps>] [--jobs <n>] [--summary] [--timings]
```

- One JSON line per document (per sweep point), deterministic unless
  `--timings` is given:

  ```json
  {"doc":"bracket.arrx","sweep":null,"status":"failed",
   "params":[{"id":"…","name":"w","value":0.012,"diagnostic":null}],
   "features":[{"id":"…","type":"core.fillet","status":"failed",
                "category":"kernel.degenerate.blend-too-large",
                "diagnostic":{"code":"…","message":"…","refs":["…"]}}],
   "bodies":[{"part":"…","feature":"…","slot":"body","volume":1.2e-5,"area":4.1e-3,
              "faces":14,"edges":36,"vertices":24}]}
  ```

- `--summary` ends with the failure histogram: one row per category with
  its count, the documents that hit it and the plugins that called it.
- `--save-fixtures` writes the failing call record of every kernel failure
  as an Arris fixture recipe (`docs/ARCHITECTURE.md` §The kernel choke
  point).
- `--jobs` evaluates documents in parallel, each with its own evaluator
  and Arris model: parallelism across documents, never inside one.
- Exit status: 0 when every document evaluated without a failed
  parameter or feature (suppressed and rolled-back ones are not
  failures), 1 otherwise, 2 on usage or I/O errors.
- **Built so far:** `arrix eval <dir>` on one document directory, no
  flags. It opens the document through `arrix-doc`, which reads no files:
  the CLI hands it a directory as a `DocumentSource`, and the line type,
  `EvalLine`, and `arrix_doc::eval` live in `arrix-doc`, so tests and the
  CLI share one code path. The document is evaluated with the core
  feature types registered (`Registry::with_core_types`). An empty
  document prints
  `{"doc":"<dir>","sweep":null,"status":"ok","params":[],"features":[],"bodies":[]}`
  (`doc` as the caller named it) and exits 0. A missing directory, a
  `document.json` that is absent or not JSON, a schema other than 1, or a
  malformed document exits 2 with the reason on stderr and nothing on
  stdout. The rest of the flags are M4's.

## The browser build

The same Rust codebase runs in the browser (`SEED.md` §1). C1 keeps it
compiling; C4 makes it run.

- **C1**: every crate but `arrix-cli` and the native plugin tiers builds
  for `wasm32-unknown-unknown` in the pre-push hook and CI. Nothing runs in
  a browser yet, and nothing is built that would make the browser path a
  special case (the wasm lint, `docs/ARCHITECTURE.md` §Gates).
- **C4**: `arrix-app` as an eframe web app. The client runs on the main
  thread; the authority and the evaluator run in a **web worker**,
  exchanging the same serialised commands and events the protocol already
  defines. The kernel never runs on the main thread, in the browser too.
- **Cancellation in the browser** needs a flag the main thread can set and
  the worker can read mid-operation: a `SharedArrayBuffer`, which requires
  a cross-origin-isolated page, or the step budget. Terminating the worker
  is the fallback and costs its cache. Chosen in C4, by ADR.
- **Files** come from the browser's file APIs as bytes; there is no
  filesystem. Importers already take bytes (`docs/PLUGINS.md` §4).
- **Plugins**: Tier 0 only. A document using a Tier 1 or Tier 2 plugin
  opens with those features frozen (`docs/DATA-MODEL.md` §Frozen results).
- **Picking** uses the same asynchronous readback as native.

## Budgets

Targets, not gates, until the C1 benchmark measures them on the reference
machine; a target becomes a gate in the plan that first meets it, with the
measured number written here.

| What | Target |
|---|---|
| A UI frame with the C1 acceptance part on screen | ≤ 16 ms |
| Applying a command (authority and replica) | ≤ 1 ms |
| One sketch-drag solve on a 200-entity sketch | ≤ 4 ms, on the UI thread |
| First coarse mesh after a parameter edit of the acceptance part | ≤ 150 ms |
| Cancellation latency once A3 lands | ≤ 50 ms |
| Save and load of the acceptance document | ≤ 100 ms each |
| `arrix eval` throughput over the scenario documents | reported, no target yet |

## Open questions

- The browser's cancellation mechanism (§The browser build). C4; ADR.
